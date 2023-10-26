use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use anyhow::{anyhow, Result};
use log::error;
use symphonia::core::audio::RawSampleBuffer;
use symphonia::core::errors::Error;
use symphonia::core::formats::{SeekMode, SeekTo};
use symphonia::core::sample::i24;
use symphonia::core::units::Time;

use crate::audio::{
    BitsPerSample, Device, DeviceTrait, Host, HostTrait, StreamContext, StreamParams, StreamingCommand,
};
use crate::song::Song;

pub struct Player {
    device_id: Option<u32>,
    device: Device,
    host: Host,
    is_playing: Arc<AtomicBool>,
}

impl Player {
    pub fn new(host: Host, device_id: Option<u32>) -> Result<Self> {
        let device = host
            .create_device(device_id)
            .map_err(|err| anyhow!(err.to_string()))?;
        Ok(Player {
            device_id,
            device,
            host,
            is_playing: Arc::new(AtomicBool::new(false)),
        })
    }

    /// Plays a FLAC file
    /// - params:
    ///    - song: song struct
    pub async fn play_song(&mut self, song: Arc<Song>) -> Result<()> {
        self.device.stop();
        if self.is_playing.load(std::sync::atomic::Ordering::Relaxed) {
            self.is_playing.store(false, std::sync::atomic::Ordering::Relaxed);
            tokio::time::sleep(tokio::time::Duration::from_millis(1000)).await;
        }
        song.format.lock().unwrap().seek(
            SeekMode::Accurate,
            SeekTo::Time {
                time: Time::default(),
                track_id: None,
            },
        )?;
        song.decoder.lock().unwrap().reset();
        let bits_per_sample = song.bits_per_sample;

        let streamparams = StreamParams {
            samplerate: song.sample,
            channels: song.channels as u8,
            bits_per_sample: song.bits_per_sample,
            buffer_length: 0,
            exclusive: true,
        };

        let device = self.device.clone();
        let is_playing = self.is_playing.clone();
        std::thread::spawn(move || {
            is_playing.store(true, std::sync::atomic::Ordering::Relaxed);
            while is_playing.load(std::sync::atomic::Ordering::Relaxed) {
                let packet = match song.format.lock().unwrap().next_packet() {
                    Ok(packet) => packet,
                    Err(Error::ResetRequired) => {
                        unimplemented!();
                    }
                    Err(Error::IoError(err)) => {
                        // Error reading packet: IoError(Custom { kind: UnexpectedEof, error: "end of stream" })
                        match err.kind() {
                            std::io::ErrorKind::UnexpectedEof => {
                                device.stop();
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

                let mut decoder = song.decoder.lock().unwrap();
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
                            device.send(*i)?;
                        }
                    }
                    BitsPerSample::Bits16 => {
                        let mut sample_buffer = RawSampleBuffer::<i16>::new(duration, *spec);
                        sample_buffer.copy_interleaved_ref(decoded);
                        for i in sample_buffer.as_bytes().iter() {
                            device.send(*i)?;
                        }
                    }
                    BitsPerSample::Bits24 => {
                        let mut sample_buffer = RawSampleBuffer::<i24>::new(duration, *spec);
                        sample_buffer.copy_interleaved_ref(decoded);
                        for i in sample_buffer.as_bytes().iter() {
                            device.send(*i)?;
                        }
                    }
                    BitsPerSample::Bits32 => {
                        let mut sample_buffer = RawSampleBuffer::<f32>::new(duration, *spec);
                        sample_buffer.copy_interleaved_ref(decoded);
                        for i in sample_buffer.as_bytes().iter() {
                            device.send(*i)?;
                        }
                    }
                };
            }
            Ok::<(), anyhow::Error>(())
        });

        let mut device = self.device.clone();
        device
            .start(StreamContext::new(streamparams))
            .map_err(|err| anyhow!(err.to_string()))?;

        Ok(())
    }
}
