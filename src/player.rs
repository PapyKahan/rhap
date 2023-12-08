use anyhow::{anyhow, Result};
use log::error;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use symphonia::core::audio::RawSampleBuffer;
use symphonia::core::errors::Error;
use symphonia::core::formats::{SeekMode, SeekTo};
use symphonia::core::sample::i24;
use symphonia::core::units::Time;
use tokio::sync::mpsc::Sender;
use tokio::task::JoinHandle;

use crate::audio::{BitsPerSample, Device, DeviceTrait, Host, HostTrait, StreamParams};
use crate::song::Song;

pub struct Player {
    current_device: Option<Device>,
    host: Host,
    device_id: Option<u32>,
    previous_stream: Option<Sender<u8>>,
    streaming_handle: Option<JoinHandle<Result<()>>>,
    is_playing: Arc<AtomicBool>,
}

#[derive(Clone)]
pub struct CurrentTrackInfo {
    is_streaming: Arc<AtomicBool>,
    pub title: String,
    pub artist: String,
}

impl CurrentTrackInfo {
    pub fn is_streaming(&self) -> bool {
        self.is_streaming.load(Ordering::Relaxed)
    }
}

impl Player {
    pub fn new(host: Host, device_id: Option<u32>) -> Result<Self> {
        Ok(Player {
            current_device: None,
            host,
            device_id,
            previous_stream: None,
            streaming_handle: None,
            is_playing: Arc::new(AtomicBool::new(false)),
        })
    }

    pub fn stop(&mut self) -> Result<()> {
        self.is_playing.store(false, Ordering::Relaxed);
        if let Some(device) = &mut self.current_device {
            device.stop()?;
        }
        if let Some(stream) = self.previous_stream.take() {
            drop(stream);
        }
        if let Some(handle) = self.streaming_handle.take() {
            handle.abort();
        }
        Ok(())
    }

    pub fn pause(&mut self) -> Result<()> {
        if let Some(device) = &mut self.current_device {
            device.pause()?;
        }
        Ok(())
    }

    /// Plays a FLAC file
    /// - params:
    ///    - song: song struct
    pub async fn play(&mut self, song: Arc<Song>) -> Result<CurrentTrackInfo> {
        let streamparams = StreamParams {
            samplerate: song.sample,
            channels: song.channels as u8,
            bits_per_sample: song.bits_per_sample,
            buffer_length: 0,
            exclusive: true,
        };
        let mut device = self
            .host
            .create_device(self.device_id)
            .map_err(|err| anyhow!(err.to_string()))?;
        let streamparams = device.adjust_stream_params(streamparams)?;
        let data_sender = device
            .start(streamparams)
            .map_err(|err| anyhow!(err.to_string()))?;
        self.current_device = Some(device);
        self.previous_stream = Some(data_sender);
        let stream = self.previous_stream.clone();
        let progress = Arc::new(AtomicU64::new(0));
        let is_streaming = Arc::new(AtomicBool::new(true));
        let report_streaming = Arc::clone(&is_streaming);
        let is_playing = self.is_playing.clone();
        let report_song = song.clone();
        self.streaming_handle = Some(tokio::spawn(async move {
            let mut format = song.format.lock().await;
            format.seek(
                SeekMode::Accurate,
                SeekTo::Time {
                    time: Time::default(),
                    track_id: None,
                },
            )?;
            let mut decoder = song.decoder.lock().await;
            decoder.reset();
            is_playing.store(true, Ordering::Relaxed);

            loop {
                if !is_playing.load(Ordering::Relaxed) {
                    break;
                }
                if let Some(ref streamer) = stream {
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
                    progress.store(
                        progress.load(Ordering::Relaxed) + packet.dur,
                        Ordering::Relaxed,
                    );

                    let decoded = decoder.decode(&packet)?;
                    let spec = decoded.spec();
                    let duration = decoded.capacity() as u64;
                    // Not very efficient, but i can't create a RawSampleBuffer dynamically
                    // so i have to create one for each possible bits_per_sample and at eatch iteration
                    match streamparams.bits_per_sample {
                        BitsPerSample::Bits8 => {
                            if song.sample != streamparams.samplerate {
                                let mut r = crate::tools::Resampler::<i8>::new(
                                    *spec,
                                    streamparams.samplerate as usize,
                                    duration,
                                );
                                if let Some(buffer) = r.resample(decoded) {
                                    for i in buffer.iter() {
                                        for j in i.to_ne_bytes().iter() {
                                            if streamer.send(*j).await.is_err() {
                                                break;
                                            }
                                        }
                                    }
                                }
                                if let Some(buffer) = r.flush() {
                                    for i in buffer.iter() {
                                        for j in i.to_ne_bytes().iter() {
                                            if streamer.send(*j).await.is_err() {
                                                break;
                                            }
                                        }
                                    }
                                }
                            } else {
                                let mut sample_buffer = RawSampleBuffer::<i8>::new(duration, *spec);
                                sample_buffer.copy_interleaved_ref(decoded);
                                for i in sample_buffer.as_bytes().iter() {
                                    if streamer.send(*i).await.is_err() {
                                        break;
                                    }
                                }
                            }
                        }
                        BitsPerSample::Bits16 => {
                            if song.sample != streamparams.samplerate {
                                let mut r = crate::tools::Resampler::<i16>::new(
                                    *spec,
                                    streamparams.samplerate as usize,
                                    duration,
                                );
                                if let Some(buffer) = r.resample(decoded) {
                                    for i in buffer.iter() {
                                        for j in i.to_ne_bytes().iter() {
                                            if streamer.send(*j).await.is_err() {
                                                break;
                                            }
                                        }
                                    }
                                }
                                if let Some(buffer) = r.flush() {
                                    for i in buffer.iter() {
                                        for j in i.to_ne_bytes().iter() {
                                            if streamer.send(*j).await.is_err() {
                                                break;
                                            }
                                        }
                                    }
                                }
                            } else {
                                let mut sample_buffer =
                                    RawSampleBuffer::<i16>::new(duration, *spec);
                                sample_buffer.copy_interleaved_ref(decoded);
                                for i in sample_buffer.as_bytes().iter() {
                                    if streamer.send(*i).await.is_err() {
                                        break;
                                    }
                                }
                            }
                        }
                        BitsPerSample::Bits24 => {
                            if song.sample != streamparams.samplerate {
                                let mut r = crate::tools::Resampler::<i24>::new(
                                    *spec,
                                    streamparams.samplerate as usize,
                                    duration,
                                );
                                if let Some(buffer) = r.resample(decoded) {
                                    for i in buffer.iter() {
                                        for j in i.to_ne_bytes().iter() {
                                            if streamer.send(*j).await.is_err() {
                                                break;
                                            }
                                        }
                                    }
                                }
                                if let Some(buffer) = r.flush() {
                                    for i in buffer.iter() {
                                        for j in i.to_ne_bytes().iter() {
                                            if streamer.send(*j).await.is_err() {
                                                break;
                                            }
                                        }
                                    }
                                }
                            } else {
                                let mut sample_buffer =
                                    RawSampleBuffer::<i24>::new(duration, *spec);
                                sample_buffer.copy_interleaved_ref(decoded);
                                for i in sample_buffer.as_bytes().iter() {
                                    if streamer.send(*i).await.is_err() {
                                        break;
                                    }
                                }
                            }
                        }
                        BitsPerSample::Bits32 => {
                            if song.sample != streamparams.samplerate {
                                let mut r = crate::tools::Resampler::<f32>::new(
                                    *spec,
                                    streamparams.samplerate as usize,
                                    duration,
                                );
                                if let Some(buffer) = r.resample(decoded) {
                                    for i in buffer.iter() {
                                        for j in i.to_ne_bytes().iter() {
                                            if streamer.send(*j).await.is_err() {
                                                break;
                                            }
                                        }
                                    }
                                }
                                if let Some(buffer) = r.flush() {
                                    for i in buffer.iter() {
                                        for j in i.to_ne_bytes().iter() {
                                            if streamer.send(*j).await.is_err() {
                                                break;
                                            }
                                        }
                                    }
                                }
                            } else {
                                let mut sample_buffer =
                                    RawSampleBuffer::<f32>::new(duration, *spec);
                                sample_buffer.copy_interleaved_ref(decoded);
                                for i in sample_buffer.as_bytes().iter() {
                                    if streamer.send(*i).await.is_err() {
                                        break;
                                    }
                                }
                            }
                        }
                    }
                };
            }
            is_streaming.store(false, Ordering::Relaxed);
            is_playing.store(false, Ordering::Relaxed);
            Ok::<(), anyhow::Error>(())
        }));

        Ok(CurrentTrackInfo {
            is_streaming: report_streaming,
            title: report_song.title.clone(),
            artist: report_song.artist.clone(),
        })
    }
}
