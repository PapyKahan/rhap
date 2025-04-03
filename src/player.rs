use anyhow::Result;
use log::error;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use symphonia::core::audio::{AudioBufferRef, RawSampleBuffer, SignalSpec};
use symphonia::core::errors::Error;
use symphonia::core::formats::{SeekMode, SeekTo};
use symphonia::core::sample::i24;
use symphonia::core::units::{Time, TimeBase};
use tokio::sync::mpsc::Sender;
use tokio::task::JoinHandle;

use crate::audio::{
    BitsPerSample, Device, DeviceTrait, Host, HostTrait, StreamParams, StreamingData,
};
use crate::musictrack::MusicTrack;
use crate::tools::resampler::RubatoResampler;

pub struct Player {
    current_device: Option<Device>,
    host: Host,
    device_id: Option<u32>,
    pollmode: bool,
    previous_stream: Option<Sender<StreamingData>>,
    streaming_handle: Option<JoinHandle<Result<()>>>,
    is_playing: Arc<AtomicBool>,
    is_paused: Arc<AtomicBool>,
}

#[derive(Clone)]
pub struct CurrentTrackInfo {
    is_streaming: Arc<AtomicBool>,
    pub title: String,
    pub artist: String,
    pub info: String,
    pub elapsed_time: Arc<AtomicU64>,
    pub total_duration: Time,
    time_base: TimeBase,
}

impl CurrentTrackInfo {
    pub fn is_streaming(&self) -> bool {
        self.is_streaming.load(Ordering::Relaxed)
    }

    pub fn get_elapsed_time(&self) -> Time {
        let elapsed = self.elapsed_time.load(Ordering::Relaxed);
        self.time_base.calc_time(elapsed)
    }

    pub fn format_time(&self, time: Time) -> String {
        let hours = time.seconds / (60 * 60);
        let mins = (time.seconds % (60 * 60)) / 60;
        let secs = time.seconds % 60;
        match hours {
            0 => match mins {
                0 => format!("00:{:0>2}", secs),
                _ => format!("{:0>2}:{:0>2}", mins, secs),
            },
            _ => format!("{}:{:0>2}:{:0>2}", hours, mins, secs),
        }
    }
}

pub enum StreamBuffer {
    I16(RawSampleBuffer<i16>),
    I24(RawSampleBuffer<i24>),
    F32(RawSampleBuffer<f32>),
}

impl StreamBuffer {
    pub fn new(bits_per_sample: BitsPerSample, duration: usize, spec: SignalSpec) -> Self {
        match bits_per_sample {
            BitsPerSample::Bits16 => {
                StreamBuffer::I16(RawSampleBuffer::<i16>::new(duration as u64, spec))
            }
            BitsPerSample::Bits24 => {
                StreamBuffer::I24(RawSampleBuffer::<i24>::new(duration as u64, spec))
            }
            BitsPerSample::Bits32 => {
                StreamBuffer::F32(RawSampleBuffer::<f32>::new(duration as u64, spec))
            }
        }
    }

    pub fn copy_interleaved_ref(&mut self, decoded: AudioBufferRef<'_>) {
        match self {
            StreamBuffer::I16(buffer) => buffer.copy_interleaved_ref(decoded),
            StreamBuffer::I24(buffer) => buffer.copy_interleaved_ref(decoded),
            StreamBuffer::F32(buffer) => buffer.copy_interleaved_ref(decoded),
        }
    }

    pub fn as_bytes(&self) -> &[u8] {
        match self {
            StreamBuffer::I16(buffer) => buffer.as_bytes(),
            StreamBuffer::I24(buffer) => buffer.as_bytes(),
            StreamBuffer::F32(buffer) => buffer.as_bytes(),
        }
    }
}

enum Resampler {
    I16(RubatoResampler<i16>),
    I24(RubatoResampler<i24>),
    F32(RubatoResampler<f32>),
}

impl Resampler {
    pub fn new(
        input_bits_per_sample: BitsPerSample,
        output_bits_per_sample: BitsPerSample,
        input_sample_rate: usize,
        output_samplerate: usize,
        frames: usize,
        channels: usize,
    ) -> Result<Self> {
        match output_bits_per_sample {
            BitsPerSample::Bits16 => Ok(Resampler::I16(RubatoResampler::<i16>::new(
                input_sample_rate,
                output_samplerate,
                input_bits_per_sample,
                output_bits_per_sample,
                frames,
                channels,
            )?)),
            BitsPerSample::Bits24 => Ok(Resampler::I24(RubatoResampler::<i24>::new(
                input_sample_rate,
                output_samplerate,
                input_bits_per_sample,
                output_bits_per_sample,
                frames,
                channels,
            )?)),
            BitsPerSample::Bits32 => Ok(Resampler::F32(RubatoResampler::<f32>::new(
                input_sample_rate,
                output_samplerate,
                input_bits_per_sample,
                output_bits_per_sample,
                frames,
                channels,
            )?)),
        }
    }

    pub async fn send_resampled_data(
        &mut self,
        streambuffer: &AudioBufferRef<'_>,
        streamer: &Sender<StreamingData>,
    ) -> Result<()> {
        match self {
            Resampler::I16(resampler) => {
                let output = resampler.resample(streambuffer)?;
                for i in output.iter() {
                    for j in i.to_ne_bytes().iter() {
                        streamer.send(StreamingData::Data(*j)).await?
                    }
                }
            }
            Resampler::I24(resampler) => {
                let output = resampler.resample(streambuffer)?;
                for i in output.iter() {
                    for j in i.to_ne_bytes().iter() {
                        streamer.send(StreamingData::Data(*j)).await?
                    }
                }
            }
            Resampler::F32(resampler) => {
                let output = resampler.resample(streambuffer)?;
                for i in output.iter() {
                    for j in i.to_ne_bytes().iter() {
                        streamer.send(StreamingData::Data(*j)).await?
                    }
                }
            }
        }
        Ok(())
    }
}

impl Player {
    pub fn new(host: Host, device_id: Option<u32>, pollmode: bool) -> Result<Self> {
        Ok(Player {
            current_device: None,
            host,
            device_id,
            pollmode,
            previous_stream: None,
            streaming_handle: None,
            is_playing: Arc::new(AtomicBool::new(false)),
            is_paused: Arc::new(AtomicBool::new(false)),
        })
    }

    pub async fn stop(&mut self) -> Result<()> {
        self.is_playing.store(false, Ordering::Relaxed);
        self.is_paused.store(false, Ordering::Relaxed);
        if let Some(mut device) = self.current_device.take() {
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
        if !self.is_paused.load(Ordering::Relaxed) {
            if let Some(device) = self.current_device.as_mut() {
                device.pause()?;
            }
            self.is_paused.store(true, Ordering::Relaxed);
        }
        Ok(())
    }

    pub fn resume(&mut self) -> Result<()> {
        if self.is_paused.load(Ordering::Relaxed) {
            if let Some(device) = self.current_device.as_mut() {
                device.resume()?;
            }
            self.is_paused.store(false, Ordering::Relaxed);
        }
        Ok(())
    }

    pub fn is_playing(&self) -> bool {
        self.is_playing.load(Ordering::Relaxed) && !self.is_paused.load(Ordering::Relaxed)
    }

    pub fn is_paused(&self) -> bool {
        self.is_paused.load(Ordering::Relaxed)
    }

    pub async fn play(&mut self, song: Arc<MusicTrack>) -> Result<CurrentTrackInfo> {
        let streamparams = StreamParams {
            samplerate: song.sample,
            channels: song.channels as u8,
            bits_per_sample: song.bits_per_sample,
            exclusive: true,
            pollmode: self.pollmode,
        };
        let mut device = self.host.create_device(self.device_id)?;
        let adjusted_params = device.adjust_stream_params(&streamparams)?;
        let data_sender = device.start(&adjusted_params)?;
        self.current_device = Some(device);
        self.previous_stream = Some(data_sender);
        let stream = self.previous_stream.clone();
        let is_streaming = Arc::new(AtomicBool::new(true));
        let report_streaming = Arc::clone(&is_streaming);
        let is_playing = self.is_playing.clone();
        let track = song.clone();
        let elapsed_time = Arc::new(AtomicU64::new(0));
        let elapsed_time_clone = Arc::clone(&elapsed_time);
        let total_duration = track.duration;
        let time_base = track.format.lock().await.tracks().get(0).unwrap().codec_params.time_base.unwrap_or(Default::default());
        self.streaming_handle = Some(tokio::spawn(async move {
            let mut format = track.format.lock().await;
            format.seek(
                SeekMode::Accurate,
                SeekTo::Time {
                    time: Time::default(),
                    track_id: None,
                },
            )?;
            let mut decoder = track.decoder.lock().await;
            decoder.reset();
            is_playing.store(true, Ordering::Relaxed);
            if let Some(streamer) = stream {
                let mut buffer: Option<StreamBuffer> = None;
                let mut resampler: Option<Resampler> = None;
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
                    elapsed_time_clone.store(
                        elapsed_time_clone.load(Ordering::Relaxed) + packet.dur,
                        Ordering::Relaxed,
                    );
                    let decoded = decoder.decode(&packet)?;
                    let spec = decoded.spec();
                    let frames = decoded.capacity();
                    let sample_buffer = buffer.get_or_insert_with(|| {
                        StreamBuffer::new(adjusted_params.bits_per_sample, frames, *spec)
                    });
                    if track.sample != adjusted_params.samplerate {
                        let resampled_sender = resampler.get_or_insert_with(|| {
                            Resampler::new(
                                streamparams.bits_per_sample,
                                adjusted_params.bits_per_sample,
                                streamparams.samplerate as usize,
                                adjusted_params.samplerate as usize,
                                frames,
                                adjusted_params.channels as usize,
                            )
                            .unwrap()
                        });
                        if resampled_sender
                            .send_resampled_data(&decoded, &streamer)
                            .await
                            .is_err()
                        {
                            break;
                        }
                    } else {
                        sample_buffer.copy_interleaved_ref(decoded);
                        for i in sample_buffer.as_bytes().iter() {
                            if streamer.send(StreamingData::Data(*i)).await.is_err() {
                                break;
                            }
                        }
                    }
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
            title: song.title.clone(),
            artist: song.artist.clone(),
            info: song.info(),
            elapsed_time,
            total_duration,
            time_base,
        })
    }
}

