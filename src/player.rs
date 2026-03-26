use anyhow::Result;
use log::error;
use ringbuf::HeapProd;
use ringbuf::traits::Producer;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicU8, Ordering};
use std::sync::Arc;
use std::time::Duration;
use symphonia::core::audio::{AudioBufferRef, RawSampleBuffer, SignalSpec};
use symphonia::core::errors::Error;
use symphonia::core::formats::{SeekMode, SeekTo};
use symphonia::core::sample::i24;
use symphonia::core::units::{Time, TimeBase};

use crate::audio::{
    BitsPerSample, Capabilities, Device, DeviceTrait, Host, HostTrait, StreamParams,
};
use crate::audio::device::BufferSignal;
use crate::musictrack::MusicTrack;
use crate::tools::resampler::RubatoResampler;

enum DecoderResult {
    EndOfTrack {
        producer: HeapProd<u8>,
        end_of_stream: Arc<AtomicBool>,
        signal: Arc<BufferSignal>,
        resampler: Option<Resampler>,
    },
    Stopped,
    Error(anyhow::Error),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum PlayerState {
    Stopped = 0,
    Playing = 1,
    Paused = 2,
}

impl PlayerState {
    fn from_u8(v: u8) -> Self {
        match v {
            1 => Self::Playing,
            2 => Self::Paused,
            _ => Self::Stopped,
        }
    }
}

pub struct AtomicPlayerState(AtomicU8);

impl AtomicPlayerState {
    pub fn new(state: PlayerState) -> Self {
        Self(AtomicU8::new(state as u8))
    }

    pub fn load(&self, ordering: Ordering) -> PlayerState {
        PlayerState::from_u8(self.0.load(ordering))
    }

    pub fn store(&self, state: PlayerState, ordering: Ordering) {
        self.0.store(state as u8, ordering);
    }

    /// Attempt an atomic state transition. Returns Ok if the current state
    /// matched `from` and was changed to `to`, Err with the actual state otherwise.
    pub fn transition(&self, from: PlayerState, to: PlayerState) -> Result<(), PlayerState> {
        self.0
            .compare_exchange(from as u8, to as u8, Ordering::AcqRel, Ordering::Acquire)
            .map(|_| ())
            .map_err(PlayerState::from_u8)
    }
}

pub struct Player {
    current_device: Option<Device>,
    host: Host,
    device_id: Option<u32>,
    pollmode: bool,
    gapless: bool,
    resample: bool,
    streaming_handle: Option<std::thread::JoinHandle<DecoderResult>>,
    state: Arc<AtomicPlayerState>,
    current_signal: Option<Arc<BufferSignal>>,
    cached_capabilities: Option<Capabilities>,
    current_adjusted_params: Option<StreamParams>,
}

#[derive(Clone)]
pub struct CurrentTrackInfo {
    is_streaming: Arc<AtomicBool>,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub info: String,
    pub output_info: Option<String>,
    pub elapsed_time: Arc<AtomicU64>,
    pub total_duration: Time,
    time_base: TimeBase,
    pub cover_art: Option<Arc<[u8]>>,
    pub cover_art_mime: Option<String>,
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

macro_rules! dispatch_buffer {
    ($self:expr, $buf:ident => $body:expr) => {
        match $self {
            StreamBuffer::I16($buf) => $body,
            StreamBuffer::I24($buf) => $body,
            StreamBuffer::F32($buf) => $body,
        }
    };
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
        dispatch_buffer!(self, buf => buf.copy_interleaved_ref(decoded));
    }

    pub fn as_bytes(&self) -> &[u8] {
        dispatch_buffer!(self, buf => buf.as_bytes())
    }
}

macro_rules! dispatch_resampler {
    ($self:expr, $res:ident, $bytes:ident => $body:expr) => {
        match $self {
            Resampler::I16($res, $bytes) => $body,
            Resampler::I24($res, $bytes) => $body,
            Resampler::F32($res, $bytes) => $body,
        }
    };
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
        macro_rules! new_resampler {
            ($variant:ident, $ty:ty) => {
                Ok(Resampler::$variant(
                    RubatoResampler::<$ty>::new(
                        input_sample_rate, output_samplerate,
                        input_bits_per_sample, output_bits_per_sample,
                        frames, channels,
                    )?,
                    Vec::new(),
                ))
            };
        }
        match output_bits_per_sample.0 {
            16 => new_resampler!(I16, i16),
            24 => new_resampler!(I24, i24),
            32 => new_resampler!(F32, f32),
            other => Err(anyhow::anyhow!("Unsupported bits per sample for resampler: {}", other)),
        }
    }

    pub fn resample_to_bytes(
        &mut self,
        streambuffer: &AudioBufferRef<'_>,
    ) -> Result<&[u8]> {
        dispatch_resampler!(self, resampler, byte_buf => {
            let output = resampler.resample(streambuffer)?;
            byte_buf.clear();
            if let Some(s) = output.first() {
                byte_buf.reserve(output.len() * s.to_ne_bytes().len());
            }
            for sample in output.iter() {
                byte_buf.extend_from_slice(&sample.to_ne_bytes());
            }
            Ok(byte_buf as &[u8])
        })
    }
}

fn write_all_blocking(producer: &mut HeapProd<u8>, data: &[u8], state: &AtomicPlayerState, signal: &BufferSignal) {
    let mut offset = 0;
    while offset < data.len() {
        if state.load(Ordering::Acquire) == PlayerState::Stopped {
            return;
        }
        let n = producer.push_slice(&data[offset..]);
        offset += n;
        if offset >= data.len() {
            // Full write completed — notify consumer once
            signal.notify();
        } else if n > 0 {
            // Partial write — notify and wait for consumer to drain
            signal.notify();
            signal.wait_timeout(Duration::from_millis(5));
        } else {
            // No progress — wait for consumer without notifying
            signal.wait_timeout(Duration::from_millis(5));
        }
    }
}

impl Player {
    pub fn new(host: Host, device_id: Option<u32>, pollmode: bool, gapless: bool, resample: bool) -> Result<Self> {
        Ok(Player {
            current_device: None,
            host,
            device_id,
            pollmode,
            gapless,
            resample,
            streaming_handle: None,
            state: Arc::new(AtomicPlayerState::new(PlayerState::Stopped)),
            current_signal: None,
            cached_capabilities: None,
            current_adjusted_params: None,
        })
    }

    pub fn stop(&mut self) -> Result<()> {
        self.state.store(PlayerState::Stopped, Ordering::Release);
        // Wake decoder immediately if blocked in write_all_blocking backpressure
        if let Some(signal) = self.current_signal.take() {
            signal.notify();
        }
        if let Some(mut device) = self.current_device.take() {
            device.stop()?;
        }
        if let Some(handle) = self.streaming_handle.take() {
            match handle.join() {
                Ok(DecoderResult::EndOfTrack { .. }) => {}
                Ok(DecoderResult::Stopped) => {}
                Ok(DecoderResult::Error(e)) => error!("Decoder thread error: {:#}", e),
                Err(_) => error!("Decoder thread panicked"),
            }
        }
        self.cached_capabilities = None;
        self.current_adjusted_params = None;
        Ok(())
    }

    pub fn pause(&mut self) -> Result<()> {
        if self.state.transition(PlayerState::Playing, PlayerState::Paused).is_ok() {
            if let Some(device) = self.current_device.as_mut() {
                device.pause()?;
            }
        }
        Ok(())
    }

    pub fn resume(&mut self) -> Result<()> {
        if self.state.transition(PlayerState::Paused, PlayerState::Playing).is_ok() {
            if let Some(device) = self.current_device.as_mut() {
                device.resume()?;
            }
        }
        Ok(())
    }

    pub fn is_playing(&self) -> bool {
        self.state.load(Ordering::Relaxed) == PlayerState::Playing
    }

    pub fn is_paused(&self) -> bool {
        self.state.load(Ordering::Relaxed) == PlayerState::Paused
    }

    fn spawn_decoder(
        &mut self,
        song: &Arc<MusicTrack>,
        src_params: StreamParams,
        adjusted_params: StreamParams,
        mut producer: HeapProd<u8>,
        end_of_stream: Arc<AtomicBool>,
        signal: Arc<BufferSignal>,
        playback_handle: Option<crate::musictrack::PlaybackHandle>,
        cached_resampler: Option<Resampler>,
    ) -> Result<CurrentTrackInfo> {
        let mut playback = match playback_handle {
            Some(h) => h,
            None => song.open_for_playback()?,
        };
        let time_base = playback.format.tracks().first()
            .and_then(|t| t.codec_params.time_base)
            .unwrap_or_default();

        let output_info = {
            let rate_changed = src_params.samplerate != adjusted_params.samplerate;
            let bits_changed = src_params.bits_per_sample != adjusted_params.bits_per_sample;
            if rate_changed || bits_changed {
                Some(format!("{} - {}", adjusted_params.bits_per_sample, adjusted_params.samplerate))
            } else {
                None
            }
        };

        let is_streaming = Arc::new(AtomicBool::new(true));
        let report_streaming = Arc::clone(&is_streaming);
        let state = Arc::clone(&self.state);
        let state_for_write = Arc::clone(&self.state);
        let elapsed_time = Arc::new(AtomicU64::new(0));
        let elapsed_time_clone = Arc::clone(&elapsed_time);
        let total_duration = song.duration;

        self.streaming_handle = Some(
            std::thread::Builder::new()
                .name("rhap-decoder".into())
                .spawn(move || -> DecoderResult {
                    let format = &mut playback.format;
                    if let Err(e) = format.seek(
                        SeekMode::Accurate,
                        SeekTo::Time {
                            time: Time::default(),
                            track_id: None,
                        },
                    ) {
                        end_of_stream.store(true, Ordering::Release);
                        is_streaming.store(false, Ordering::Relaxed);
                        return DecoderResult::Error(e.into());
                    }
                    let decoder = &mut playback.decoder;
                    decoder.reset();

                    let mut buffer: Option<StreamBuffer> = None;
                    let mut resampler: Option<Resampler> = cached_resampler;

                    let result: Result<()> = (|| {
                        loop {
                            if state.load(Ordering::Acquire) == PlayerState::Stopped {
                                break;
                            }
                            let packet = match format.next_packet() {
                                Ok(packet) => packet,
                                Err(Error::ResetRequired) => {
                                    decoder.reset();
                                    continue;
                                }
                                Err(Error::IoError(err)) => match err.kind() {
                                    std::io::ErrorKind::UnexpectedEof => break,
                                    _ => return Err(anyhow::anyhow!("Error reading packet: {:?}", err)),
                                },
                                Err(err) => return Err(anyhow::anyhow!("Error reading packet: {:?}", err)),
                            };
                            let decoded = decoder.decode(&packet)?;
                            elapsed_time_clone.fetch_add(packet.dur, Ordering::Relaxed);
                            let spec = decoded.spec();
                            let frames = decoded.capacity();
                            let sample_buffer = buffer.get_or_insert_with(|| {
                                StreamBuffer::new(adjusted_params.bits_per_sample, frames, *spec)
                            });

                            if src_params.samplerate != adjusted_params.samplerate {
                                if resampler.is_none() {
                                    resampler = Some(Resampler::new(
                                        src_params.bits_per_sample,
                                        adjusted_params.bits_per_sample,
                                        src_params.samplerate.0 as usize,
                                        adjusted_params.samplerate.0 as usize,
                                        frames,
                                        adjusted_params.channels as usize,
                                    )?);
                                }
                                let resampled = resampler.as_mut().unwrap();
                                let bytes = resampled.resample_to_bytes(&decoded)?;
                                write_all_blocking(&mut producer, bytes, &state_for_write, &signal);
                            } else {
                                sample_buffer.copy_interleaved_ref(decoded);
                                let bytes = sample_buffer.as_bytes();
                                write_all_blocking(&mut producer, bytes, &state_for_write, &signal);
                            }
                        }
                        Ok(())
                    })();

                    is_streaming.store(false, Ordering::Relaxed);
                    match result {
                        Ok(()) if state.load(Ordering::Acquire) != PlayerState::Stopped => {
                            DecoderResult::EndOfTrack { producer, end_of_stream, signal, resampler }
                        }
                        Ok(()) => {
                            end_of_stream.store(true, Ordering::Release);
                            DecoderResult::Stopped
                        }
                        Err(e) => {
                            end_of_stream.store(true, Ordering::Release);
                            DecoderResult::Error(e)
                        }
                    }
                })?,
        );

        Ok(CurrentTrackInfo {
            is_streaming: report_streaming,
            title: song.title.clone(),
            artist: song.artist.clone(),
            album: song.album.clone(),
            info: song.info(),
            output_info,
            elapsed_time,
            total_duration,
            time_base,
            cover_art: song.cover_art.clone(),
            cover_art_mime: song.cover_art_mime.clone(),
        })
    }

    /// Play a track with a pre-opened PlaybackHandle, avoiding a second file probe.
    pub fn play_with_handle(
        &mut self,
        song: Arc<MusicTrack>,
        handle: crate::musictrack::PlaybackHandle,
    ) -> Result<CurrentTrackInfo> {
        self.play_inner(song, Some(handle))
    }

    pub fn play(&mut self, song: Arc<MusicTrack>) -> Result<CurrentTrackInfo> {
        self.play_inner(song, None)
    }

    fn play_inner(
        &mut self,
        song: Arc<MusicTrack>,
        handle: Option<crate::musictrack::PlaybackHandle>,
    ) -> Result<CurrentTrackInfo> {
        self.stop()?;

        let src_params = StreamParams {
            samplerate: song.sample,
            channels: song.channels as u8,
            bits_per_sample: song.bits_per_sample,
            exclusive: true,
            pollmode: self.pollmode,
        };
        let mut device = self.host.create_device(self.device_id)?;
        let capabilities = device.get_capabilities()?;
        let adjusted_params = src_params.adjust_with_capabilities(&capabilities);

        if !self.resample {
            if adjusted_params.samplerate != src_params.samplerate {
                anyhow::bail!(
                    "Device does not support {} natively (would resample to {}). Use --resample to allow.",
                    src_params.samplerate, adjusted_params.samplerate,
                );
            }
            if adjusted_params.bits_per_sample < src_params.bits_per_sample {
                anyhow::bail!(
                    "Device does not support {} natively (would downconvert to {}). Use --resample to allow.",
                    src_params.bits_per_sample, adjusted_params.bits_per_sample,
                );
            }
        }

        let pipeline = device.start(&adjusted_params)?;
        self.current_device = Some(device);
        self.cached_capabilities = Some(capabilities);
        self.current_adjusted_params = Some(adjusted_params);
        self.current_signal = Some(Arc::clone(&pipeline.signal));

        self.state.store(PlayerState::Playing, Ordering::Release);

        self.spawn_decoder(
            &song,
            src_params,
            adjusted_params,
            pipeline.producer,
            pipeline.end_of_stream,
            pipeline.signal,
            handle,
            None,
        )
    }

    pub fn play_gapless(&mut self, song: Arc<MusicTrack>) -> Result<Option<CurrentTrackInfo>> {
        if !self.gapless {
            return Ok(None);
        }

        let (caps, current_adj) = match (&self.cached_capabilities, &self.current_adjusted_params) {
            (Some(c), Some(p)) => (c.clone(), *p),
            _ => return Ok(None),
        };

        let src_params = StreamParams {
            samplerate: song.sample,
            channels: song.channels as u8,
            bits_per_sample: song.bits_per_sample,
            exclusive: true,
            pollmode: self.pollmode,
        };
        if src_params.adjust_with_capabilities(&caps) != current_adj {
            return Ok(None);
        }

        let handle = match self.streaming_handle.as_ref() {
            Some(h) if h.is_finished() => self.streaming_handle.take().unwrap(),
            _ => return Ok(None),
        };
        let (producer, eos, signal, cached_resampler) = match handle.join() {
            Ok(DecoderResult::EndOfTrack { producer, end_of_stream, signal, resampler }) => {
                (producer, end_of_stream, signal, resampler)
            }
            Ok(DecoderResult::Stopped) => return Ok(None),
            Ok(DecoderResult::Error(_)) => return Ok(None),
            Err(_) => return Ok(None),
        };

        eos.store(false, Ordering::Release);
        self.current_signal = Some(Arc::clone(&signal));
        let info = self.spawn_decoder(&song, src_params, current_adj, producer, eos, signal, None, cached_resampler)?;
        Ok(Some(info))
    }
}
