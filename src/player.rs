use anyhow::Result;
use log::error;
use ringbuf::HeapProd;
use ringbuf::traits::Producer;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;
use symphonia::core::audio::{AudioBufferRef, RawSampleBuffer, SignalSpec};
use symphonia::core::errors::Error;
use symphonia::core::formats::{SeekMode, SeekTo};
use symphonia::core::sample::i24;
use symphonia::core::units::{Time, TimeBase};

use crate::audio::{
    BitsPerSample, Device, DeviceTrait, Host, HostTrait, StreamParams,
};
use crate::musictrack::MusicTrack;
use crate::tools::resampler::RubatoResampler;

pub struct Player {
    current_device: Option<Device>,
    host: Host,
    device_id: Option<u32>,
    pollmode: bool,
    streaming_handle: Option<std::thread::JoinHandle<Result<()>>>,
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

pub fn format_time(time: Time) -> String {
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

impl CurrentTrackInfo {
    pub fn is_streaming(&self) -> bool {
        self.is_streaming.load(Ordering::Relaxed)
    }

    pub fn get_elapsed_time(&self) -> Time {
        let elapsed = self.elapsed_time.load(Ordering::Relaxed);
        self.time_base.calc_time(elapsed)
    }
}

pub enum StreamBuffer {
    I16(RawSampleBuffer<i16>),
    I24(RawSampleBuffer<i24>),
    F32(RawSampleBuffer<f32>),
}

impl StreamBuffer {
    pub fn new(bits_per_sample: BitsPerSample, duration: usize, spec: SignalSpec) -> Self {
        match bits_per_sample.0 {
            16 => StreamBuffer::I16(RawSampleBuffer::<i16>::new(duration as u64, spec)),
            24 => StreamBuffer::I24(RawSampleBuffer::<i24>::new(duration as u64, spec)),
            32 => StreamBuffer::F32(RawSampleBuffer::<f32>::new(duration as u64, spec)),
            other => panic!("Unsupported bits per sample for stream buffer: {}", other),
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
    I16(RubatoResampler<i16>, Vec<u8>),
    I24(RubatoResampler<i24>, Vec<u8>),
    F32(RubatoResampler<f32>, Vec<u8>),
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
        match output_bits_per_sample.0 {
            16 => Ok(Resampler::I16(
                RubatoResampler::<i16>::new(
                    input_sample_rate,
                    output_samplerate,
                    input_bits_per_sample,
                    output_bits_per_sample,
                    frames,
                    channels,
                )?,
                Vec::new(),
            )),
            24 => Ok(Resampler::I24(
                RubatoResampler::<i24>::new(
                    input_sample_rate,
                    output_samplerate,
                    input_bits_per_sample,
                    output_bits_per_sample,
                    frames,
                    channels,
                )?,
                Vec::new(),
            )),
            32 => Ok(Resampler::F32(
                RubatoResampler::<f32>::new(
                    input_sample_rate,
                    output_samplerate,
                    input_bits_per_sample,
                    output_bits_per_sample,
                    frames,
                    channels,
                )?,
                Vec::new(),
            )),
            other => Err(anyhow::anyhow!("Unsupported bits per sample for resampler: {}", other)),
        }
    }

    pub fn resample_to_bytes(
        &mut self,
        streambuffer: &AudioBufferRef<'_>,
    ) -> Result<&[u8]> {
        match self {
            Resampler::I16(resampler, byte_buf) => {
                let output = resampler.resample(streambuffer)?;
                byte_buf.clear();
                byte_buf.reserve(output.len() * 2);
                for sample in output.iter() {
                    byte_buf.extend_from_slice(&sample.to_ne_bytes());
                }
                Ok(byte_buf)
            }
            Resampler::I24(resampler, byte_buf) => {
                let output = resampler.resample(streambuffer)?;
                byte_buf.clear();
                byte_buf.reserve(output.len() * 3);
                for sample in output.iter() {
                    byte_buf.extend_from_slice(&sample.to_ne_bytes());
                }
                Ok(byte_buf)
            }
            Resampler::F32(resampler, byte_buf) => {
                let output = resampler.resample(streambuffer)?;
                byte_buf.clear();
                byte_buf.reserve(output.len() * 4);
                for sample in output.iter() {
                    byte_buf.extend_from_slice(&sample.to_ne_bytes());
                }
                Ok(byte_buf)
            }
        }
    }
}

fn write_all_blocking(producer: &mut HeapProd<u8>, data: &[u8], is_playing: &AtomicBool) {
    let mut offset = 0;
    while offset < data.len() {
        if !is_playing.load(Ordering::Relaxed) {
            return;
        }
        let n = producer.push_slice(&data[offset..]);
        offset += n;
        if offset < data.len() {
            std::thread::sleep(Duration::from_micros(500));
        }
    }
}

impl Player {
    pub fn new(host: Host, device_id: Option<u32>, pollmode: bool) -> Result<Self> {
        Ok(Player {
            current_device: None,
            host,
            device_id,
            pollmode,
            streaming_handle: None,
            is_playing: Arc::new(AtomicBool::new(false)),
            is_paused: Arc::new(AtomicBool::new(false)),
        })
    }

    pub fn stop(&mut self) -> Result<()> {
        self.is_playing.store(false, Ordering::Release);
        self.is_paused.store(false, Ordering::Relaxed);
        if let Some(handle) = self.streaming_handle.take() {
            let _ = handle.join();
        }
        if let Some(mut device) = self.current_device.take() {
            device.stop()?;
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

    pub fn play(&mut self, song: Arc<MusicTrack>) -> Result<CurrentTrackInfo> {
        self.stop()?;

        let mut playback = song.open_for_playback()?;
        let streamparams = StreamParams {
            samplerate: song.sample,
            channels: song.channels as u8,
            bits_per_sample: song.bits_per_sample,
            exclusive: true,
            pollmode: self.pollmode,
        };
        let mut device = self.host.create_device(self.device_id)?;
        let adjusted_params = device.adjust_stream_params(&streamparams)?;

        let time_base = playback.format.tracks().get(0).unwrap().codec_params.time_base.unwrap_or(Default::default());

        let pipeline = device.start(&adjusted_params)?;
        self.current_device = Some(device);

        let is_streaming = Arc::new(AtomicBool::new(true));
        let report_streaming = Arc::clone(&is_streaming);
        let is_playing = Arc::clone(&self.is_playing);
        let elapsed_time = Arc::new(AtomicU64::new(0));
        let elapsed_time_clone = Arc::clone(&elapsed_time);
        let total_duration = song.duration;

        let mut producer = pipeline.producer;
        let end_of_stream = pipeline.end_of_stream;
        let is_playing_for_write = Arc::clone(&is_playing);

        self.is_playing.store(true, Ordering::Release);

        self.streaming_handle = Some(
            std::thread::Builder::new()
                .name("rhap-decoder".into())
                .spawn(move || -> Result<()> {
                    let format = &mut playback.format;
                    format.seek(
                        SeekMode::Accurate,
                        SeekTo::Time {
                            time: Time::default(),
                            track_id: None,
                        },
                    )?;
                    let decoder = &mut playback.decoder;
                    decoder.reset();

                    let mut buffer: Option<StreamBuffer> = None;
                    let mut resampler: Option<Resampler> = None;

                    loop {
                        if !is_playing.load(Ordering::Relaxed) {
                            break;
                        }
                        let packet = match format.next_packet() {
                            Ok(packet) => packet,
                            Err(Error::ResetRequired) => {
                                decoder.reset();
                                continue;
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
                        elapsed_time_clone.fetch_add(packet.dur, Ordering::Relaxed);
                        let decoded = decoder.decode(&packet)?;
                        let spec = decoded.spec();
                        let frames = decoded.capacity();
                        let sample_buffer = buffer.get_or_insert_with(|| {
                            StreamBuffer::new(adjusted_params.bits_per_sample, frames, *spec)
                        });

                        if streamparams.samplerate != adjusted_params.samplerate {
                            let resampled = resampler.get_or_insert_with(|| {
                                Resampler::new(
                                    streamparams.bits_per_sample,
                                    adjusted_params.bits_per_sample,
                                    streamparams.samplerate.0 as usize,
                                    adjusted_params.samplerate.0 as usize,
                                    frames,
                                    adjusted_params.channels as usize,
                                )
                                .unwrap()
                            });
                            let bytes = resampled.resample_to_bytes(&decoded)?;
                            write_all_blocking(&mut producer, bytes, &is_playing_for_write);
                        } else {
                            sample_buffer.copy_interleaved_ref(decoded);
                            let bytes = sample_buffer.as_bytes();
                            write_all_blocking(&mut producer, bytes, &is_playing_for_write);
                        }
                    }

                    end_of_stream.store(true, Ordering::Release);
                    is_streaming.store(false, Ordering::Relaxed);
                    is_playing.store(false, Ordering::Relaxed);
                    Ok::<(), anyhow::Error>(())
                })?,
        );

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
