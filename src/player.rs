use anyhow::Result;
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

use crate::audio::{
    BitsPerSample, Device, DeviceTrait, Host, HostTrait, StreamParams, StreamingData,
};
use crate::song::Song;
use crate::tools::ResamplerUtil;

pub struct Player {
    current_device: Option<Device>,
    host: Host,
    device_id: Option<u32>,
    previous_stream: Option<Sender<StreamingData>>,
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

    pub async fn stop(&mut self) -> Result<()> {
        self.is_playing.store(false, Ordering::Relaxed);
        if let Some(device) = &mut self.current_device {
            device.stop()?;
        }
        if let Some(stream) = self.previous_stream.take() {
            stream.closed().await;
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
            exclusive: true,
            pollmode: false
        };
        let mut device = self.host.create_device(self.device_id)?;
        let streamparams = device.adjust_stream_params(&streamparams)?;
        let data_sender = device.start(&streamparams)?;
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

            let mut previous_duration = u64::default();
            if let Some(streamer) = stream {
                loop {
                    if !is_playing.load(Ordering::Relaxed) {
                        break;
                    }
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
                    match streamparams.bits_per_sample {
                        BitsPerSample::Bits8 => {
                            let mut resampler: Option<ResamplerUtil<i8>> = None;
                            if previous_duration != duration {
                                previous_duration = duration;
                                resampler = None;
                            }
                            if song.sample != streamparams.samplerate {
                                let r = resampler.get_or_insert_with(|| {
                                    crate::tools::ResamplerUtil::<i8>::new(
                                        spec,
                                        streamparams.samplerate as usize,
                                        duration,
                                    )
                                });
                                if let Some(buffer) = r.resample(decoded) {
                                    for i in buffer.iter() {
                                        for j in i.to_ne_bytes().iter() {
                                            if streamer.send(StreamingData::Data(*j)).await.is_err()
                                            {
                                                break;
                                            }
                                        }
                                    }
                                }
                                if let Some(buffer) = r.flush() {
                                    for i in buffer.iter() {
                                        for j in i.to_ne_bytes().iter() {
                                            if streamer.send(StreamingData::Data(*j)).await.is_err()
                                            {
                                                break;
                                            }
                                        }
                                    }
                                }
                            } else {
                                let mut sample_buffer = RawSampleBuffer::<i8>::new(duration, *spec);
                                sample_buffer.copy_interleaved_ref(decoded);
                                for i in sample_buffer.as_bytes().iter() {
                                    if streamer.send(StreamingData::Data(*i)).await.is_err() {
                                        break;
                                    }
                                }
                            }
                        }
                        BitsPerSample::Bits16 => {
                            let mut resampler: Option<ResamplerUtil<i16>> = None;
                            if song.sample != streamparams.samplerate {
                                let r = resampler.get_or_insert_with(|| {
                                    crate::tools::ResamplerUtil::<i16>::new(
                                        spec,
                                        streamparams.samplerate as usize,
                                        duration,
                                    )
                                });
                                if let Some(buffer) = r.resample(decoded) {
                                    for i in buffer.iter() {
                                        for j in i.to_ne_bytes().iter() {
                                            if streamer.send(StreamingData::Data(*j)).await.is_err()
                                            {
                                                break;
                                            }
                                        }
                                    }
                                }
                                if let Some(buffer) = r.flush() {
                                    for i in buffer.iter() {
                                        for j in i.to_ne_bytes().iter() {
                                            if streamer.send(StreamingData::Data(*j)).await.is_err()
                                            {
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
                                    if streamer.send(StreamingData::Data(*i)).await.is_err() {
                                        break;
                                    }
                                }
                            }
                        }
                        BitsPerSample::Bits24 => {
                            let mut resampler: Option<ResamplerUtil<i24>> = None;
                            if song.sample != streamparams.samplerate {
                                let r = resampler.get_or_insert_with(|| {
                                    crate::tools::ResamplerUtil::<i24>::new(
                                        spec,
                                        streamparams.samplerate as usize,
                                        duration,
                                    )
                                });
                                if let Some(buffer) = r.resample(decoded) {
                                    for i in buffer.iter() {
                                        for j in i.to_ne_bytes().iter() {
                                            if streamer.send(StreamingData::Data(*j)).await.is_err()
                                            {
                                                break;
                                            }
                                        }
                                    }
                                }
                                if let Some(buffer) = r.flush() {
                                    for i in buffer.iter() {
                                        for j in i.to_ne_bytes().iter() {
                                            if streamer.send(StreamingData::Data(*j)).await.is_err()
                                            {
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
                                    if streamer.send(StreamingData::Data(*i)).await.is_err() {
                                        break;
                                    }
                                }
                            }
                        }
                        BitsPerSample::Bits32 => {
                            let mut resampler: Option<ResamplerUtil<f32>> = None;
                            if song.sample != streamparams.samplerate {
                                let r = resampler.get_or_insert_with(|| {
                                    crate::tools::ResamplerUtil::<f32>::new(
                                        spec,
                                        streamparams.samplerate as usize,
                                        duration,
                                    )
                                });
                                if let Some(buffer) = r.resample(decoded) {
                                    for i in buffer.iter() {
                                        for j in i.to_ne_bytes().iter() {
                                            if streamer.send(StreamingData::Data(*j)).await.is_err()
                                            {
                                                break;
                                            }
                                        }
                                    }
                                }
                                if let Some(buffer) = r.flush() {
                                    for i in buffer.iter() {
                                        for j in i.to_ne_bytes().iter() {
                                            if streamer.send(StreamingData::Data(*j)).await.is_err()
                                            {
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
                                    if streamer.send(StreamingData::Data(*i)).await.is_err() {
                                        break;
                                    }
                                }
                            }
                        }
                    };
                }
                streamer.send(StreamingData::EndOfStream).await?;
                streamer.closed().await;
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
