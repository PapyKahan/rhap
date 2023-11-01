use anyhow::{anyhow, Result};
use log::error;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::SyncSender;
use std::sync::Arc;
use symphonia::core::audio::RawSampleBuffer;
use symphonia::core::errors::Error;
use symphonia::core::formats::{SeekMode, SeekTo};
use symphonia::core::sample::i24;
use symphonia::core::units::Time;

use crate::audio::{
    BitsPerSample, Device, DeviceTrait, Host, HostTrait, StreamParams, StreamingCommand,
};
use crate::song::Song;

pub struct Player {
    device: Device,
    previous_stream: Option<SyncSender<StreamingCommand>>,
    is_streaming: Arc<AtomicBool>,
}

impl Player {
    pub fn new(host: Host, device_id: Option<u32>) -> Result<Self> {
        let device = host
            .create_device(device_id)
            .map_err(|err| anyhow!(err.to_string()))?;
        Ok(Player {
            device,
            previous_stream: None,
            is_streaming: Arc::new(AtomicBool::new(false)),
        })
    }

    pub fn stop(&mut self) -> Result<()> {
        self.is_streaming.store(false, Ordering::Relaxed);
        if let Some(stream) = self.previous_stream.take() {
            stream.send(StreamingCommand::Stop)?;
            self.device.stop()?;
            drop(stream);
        }
        Ok(())
    }

    /// Plays a FLAC file
    /// - params:
    ///    - song: song struct
    pub fn play(&mut self, song: Arc<Song>) -> Result<()> {
        self.is_streaming.store(true, Ordering::Relaxed);
        self.stop()?;
        let bits_per_sample = song.bits_per_sample;
        let streamparams = StreamParams {
            samplerate: song.sample,
            channels: song.channels as u8,
            bits_per_sample: song.bits_per_sample,
            buffer_length: 0,
            exclusive: true,
        };
        self.previous_stream = Some(
            self.device
                .start(streamparams)
                .map_err(|err| anyhow!(err.to_string()))?,
        );
        let is_streaming = self.is_streaming.clone();
        let stream = self.previous_stream.clone();
        std::thread::spawn(move || {
            is_streaming.store(true, Ordering::Relaxed);
            if let Ok(mut format) = song.format.lock() {
                format.seek(
                    SeekMode::Accurate,
                    SeekTo::Time {
                        time: Time::default(),
                        track_id: None,
                    },
                )?;
            } else {
                return Err(anyhow!("Fail to lock track format"));
            }
            if let Ok(mut decoder) = song.decoder.lock() {
                decoder.reset();
            } else {
                return Err(anyhow!("Fail to lock track decoder"));
            }
            loop {
                if let Some(ref streamer) = stream {
                    let mut format = if let Ok(format) = song.format.lock() {
                        format
                    } else {
                        return Err(anyhow!("Fail to lock format"));
                    };

                    let packet = match format.next_packet() {
                        Ok(packet) => packet,
                        Err(Error::ResetRequired) => {
                            unimplemented!();
                        }
                        Err(Error::IoError(err)) => {
                            // Error reading packet: IoError(Custom { kind: UnexpectedEof, error: "end of stream" })
                            match err.kind() {
                                std::io::ErrorKind::UnexpectedEof => {
                                    break;
                                }
                                _ => {
                                    error!("Error reading packet: {:?}", err);
                                    break;
                                }
                            }
                        }
                        Err(err) => {
                            error!("Error reading packet: {:?}", err);
                            break;
                        }
                    };

                    let mut decoder = if let Ok(decoder) = song.decoder.lock() {
                        decoder
                    } else {
                        return Err(anyhow!("Fail to lock format"));
                    };

                    let decoded = decoder.decode(&packet)?;
                    let spec = decoded.spec();
                    let duration = decoded.capacity() as u64;
                    // Not very efficient, but i can't create a RawSampleBuffer dynamically
                    // so i have to create one for each possible bits_per_sample and at eatch iteration
                    match bits_per_sample {
                        BitsPerSample::Bits8 => {
                            let mut sample_buffer = RawSampleBuffer::<u8>::new(duration, *spec);
                            sample_buffer.copy_interleaved_ref(decoded);
                            for i in sample_buffer.as_bytes().iter() {
                                if streamer
                                    .send(crate::audio::StreamingCommand::Data(*i))
                                    .is_err()
                                {
                                    break;
                                }
                            }
                        }
                        BitsPerSample::Bits16 => {
                            let mut sample_buffer = RawSampleBuffer::<i16>::new(duration, *spec);
                            sample_buffer.copy_interleaved_ref(decoded);
                            for i in sample_buffer.as_bytes().iter() {
                                if streamer
                                    .send(crate::audio::StreamingCommand::Data(*i))
                                    .is_err()
                                {
                                    break;
                                }
                            }
                        }
                        BitsPerSample::Bits24 => {
                            let mut sample_buffer = RawSampleBuffer::<i24>::new(duration, *spec);
                            sample_buffer.copy_interleaved_ref(decoded);
                            for i in sample_buffer.as_bytes().iter() {
                                if streamer
                                    .send(crate::audio::StreamingCommand::Data(*i))
                                    .is_err()
                                {
                                    break;
                                }
                            }
                        }
                        BitsPerSample::Bits32 => {
                            let mut sample_buffer = RawSampleBuffer::<f32>::new(duration, *spec);
                            sample_buffer.copy_interleaved_ref(decoded);
                            for i in sample_buffer.as_bytes().iter() {
                                if streamer
                                    .send(crate::audio::StreamingCommand::Data(*i))
                                    .is_err()
                                {
                                    break;
                                }
                            }
                        }
                    }
                };
            }
            is_streaming.store(false, Ordering::Relaxed);
            Ok::<(), anyhow::Error>(())
        });

        Ok(())
    }

    pub fn is_streaming(&self) -> bool {
        self.is_streaming.load(Ordering::Relaxed)
    }
}
