use alsa::pcm::{Access, Format, HwParams, PCM};
use alsa::{Direction, ValueOr};
use anyhow::{anyhow, Result};
use log::warn;

use crate::audio::{BitsPerSample, BufferConfig, Capabilities, SampleRate, StreamParams};

/// Classified failure of `AlsaPcm::open_classified`. Caller drives retry policy.
pub(crate) enum AlsaInitError {
    /// Transient: device held by another client (PulseAudio, PipeWire, app).
    /// Caller should sleep and retry.
    Busy,
    /// Permanent: cannot succeed by retrying as-is.
    Permanent(anyhow::Error),
}

impl AlsaInitError {
    fn from_alsa_error(err: alsa::Error, phase: &'static str) -> Self {
        match err.errno() {
            libc::EBUSY | libc::EAGAIN => Self::Busy,
            _ => Self::Permanent(anyhow!("alsa: {}: {}", phase, err)),
        }
    }
}

/// Map a bit depth to the corresponding ALSA PCM format.
fn bits_to_format(bits: BitsPerSample) -> Result<Format> {
    match bits.0 {
        16 => Ok(Format::S16LE),
        // Symphonia outputs packed 24-bit (3 bytes/sample) → S24_3LE
        24 => Ok(Format::S243LE),
        // 32-bit uses IEEE float
        32 => Ok(Format::FloatLE),
        other => Err(anyhow!("Unsupported bit depth: {}", other)),
    }
}

/// Low-level ALSA PCM wrapper.
pub struct AlsaPcm {
    pcm: PCM,
    period_bytes: usize,
    buffer_bytes: usize,
    frame_bytes: usize,
}

// SAFETY: PCM is a POSIX file descriptor-based handle. It is moved into the audio
// thread and never shared concurrently. ALSA's thread-safety documentation permits
// calling PCM functions from a single dedicated thread.
unsafe impl Send for AlsaPcm {}

impl AlsaPcm {
    /// Open and configure an ALSA PCM device for playback.
    /// Returns a classified error so the caller can drive retry policy.
    pub(crate) fn open_classified(
        device_name: &str,
        params: &StreamParams,
        buffer: &BufferConfig,
    ) -> std::result::Result<Self, AlsaInitError> {
        // F3 pre-flight: for raw hw devices, refuse before opening if the
        // /dev node doesn't exist. Surfaces unplugged USB DACs with a clear
        // message instead of a generic "No such file or directory".
        if let Some((card, dev)) = parse_hw_card_dev(device_name) {
            let dev_path = format!("/dev/snd/pcmC{}D{}p", card, dev);
            if !std::path::Path::new(&dev_path).exists() {
                return Err(AlsaInitError::Permanent(anyhow!(
                    "alsa: hw device {} not present at {} (check connection)",
                    device_name, dev_path
                )));
            }
        }

        let pcm = PCM::new(device_name, Direction::Playback, false)
            .map_err(|e| AlsaInitError::from_alsa_error(e, "open"))?;

        let format = bits_to_format(params.bits_per_sample)
            .map_err(AlsaInitError::Permanent)?;
        let channels = params.channels as u32;
        let rate = params.samplerate.0;

        // HwParams and SwParams borrow `pcm`, so configure inside a block
        // to drop the borrows before moving `pcm` into Self.
        let (period_bytes, buffer_bytes, frame_bytes) = {
            let hwp = HwParams::any(&pcm)
                .map_err(|e| AlsaInitError::from_alsa_error(e, "hw_params_any"))?;

            hwp.set_access(Access::RWInterleaved)
                .map_err(|e| AlsaInitError::from_alsa_error(e, "set_access"))?;
            hwp.set_format(format)
                .map_err(|e| AlsaInitError::from_alsa_error(e, "set_format"))?;
            hwp.set_rate(rate, ValueOr::Nearest)
                .map_err(|e| AlsaInitError::from_alsa_error(e, "set_rate"))?;
            hwp.set_channels(channels)
                .map_err(|e| AlsaInitError::from_alsa_error(e, "set_channels"))?;

            let target_period_us: u32 = buffer.device_period_ms.saturating_mul(1_000);
            hwp.set_period_time_near(target_period_us, ValueOr::Nearest)
                .map_err(|e| AlsaInitError::from_alsa_error(e, "set_period_time_near"))?;
            hwp.set_periods(4, ValueOr::Nearest)
                .map_err(|e| AlsaInitError::from_alsa_error(e, "set_periods"))?;

            pcm.hw_params(&hwp)
                .map_err(|e| AlsaInitError::from_alsa_error(e, "apply hw_params"))?;

            let actual_period_frames = hwp.get_period_size()
                .map_err(|e| AlsaInitError::from_alsa_error(e, "get_period_size"))?;
            let actual_buffer_frames = hwp.get_buffer_size()
                .map_err(|e| AlsaInitError::from_alsa_error(e, "get_buffer_size"))?;
            let actual_rate = hwp.get_rate()
                .map_err(|e| AlsaInitError::from_alsa_error(e, "get_rate"))?;
            let actual_channels = hwp.get_channels()
                .map_err(|e| AlsaInitError::from_alsa_error(e, "get_channels"))?;

            let bytes_per_sample = params.bits_per_sample.0 as usize / 8;
            let frame_bytes = actual_channels as usize * bytes_per_sample;
            let period_bytes = actual_period_frames as usize * frame_bytes;
            let buffer_bytes = actual_buffer_frames as usize * frame_bytes;

            let swp = pcm.sw_params_current()
                .map_err(|e| AlsaInitError::from_alsa_error(e, "sw_params_current"))?;
            swp.set_start_threshold(actual_period_frames as alsa::pcm::Frames)
                .map_err(|e| AlsaInitError::from_alsa_error(e, "set_start_threshold"))?;
            swp.set_avail_min(actual_period_frames as alsa::pcm::Frames)
                .map_err(|e| AlsaInitError::from_alsa_error(e, "set_avail_min"))?;
            pcm.sw_params(&swp)
                .map_err(|e| AlsaInitError::from_alsa_error(e, "apply sw_params"))?;

            // F4: explicit prepare guarantees PCM_STATE_PREPARED before first writei,
            // even on drivers that don't auto-prepare after hw_params.
            pcm.prepare()
                .map_err(|e| AlsaInitError::from_alsa_error(e, "prepare"))?;

            let period_time_us = if actual_rate > 0 {
                (actual_period_frames as f64 / actual_rate as f64) * 1_000_000.0
            } else {
                0.0
            };
            let buffer_time_us = if actual_rate > 0 {
                (actual_buffer_frames as f64 / actual_rate as f64) * 1_000_000.0
            } else {
                0.0
            };
            log::info!(
                "ALSA opened: device={}, rate={}, channels={}, period={} frames ({:.0}us), buffer={} frames ({:.0}us)",
                device_name, actual_rate, actual_channels,
                actual_period_frames, period_time_us,
                actual_buffer_frames, buffer_time_us,
            );
            if period_time_us > 0.0 && period_time_us < 3000.0 {
                log::warn!(
                    "ALSA period time ({:.0}us) is very low — may cause XRUNs on some devices",
                    period_time_us
                );
            }

            (period_bytes, buffer_bytes, frame_bytes)
        };

        Ok(Self {
            pcm,
            period_bytes,
            buffer_bytes,
            frame_bytes,
        })
    }


    /// Write interleaved PCM bytes to the device, with XRUN recovery.
    /// Handles short writes by retrying with the remaining data.
    pub fn write(&self, data: &[u8]) -> Result<()> {
        let mut offset = 0;
        while offset < data.len() {
            let io = self.pcm.io_bytes();
            match io.writei(&data[offset..]) {
                Ok(n) => {
                    offset += n * self.frame_bytes;
                }
                Err(e) => {
                    self.pcm
                        .try_recover(e, false)
                        .map_err(|e2| anyhow!("ALSA xrun recovery failed: {}", e2))?;
                }
            }
        }
        Ok(())
    }

    /// Block until all buffered audio has been played out.
    pub fn drain(&self) -> Result<()> {
        self.pcm
            .drain()
            .map_err(|e| anyhow!("ALSA drain failed: {}", e))
    }

    /// Pause playback (uses ALSA pause; falls back to drop+prepare if unsupported).
    pub fn pause(&self) -> Result<()> {
        if self.pcm.pause(true).is_err() {
            let _ = self.pcm.drop();
            let _ = self.pcm.prepare();
        }
        Ok(())
    }

    /// Resume playback after pause.
    pub fn resume(&self) -> Result<()> {
        if self.pcm.pause(false).is_err() {
            let _ = self.pcm.prepare();
        }
        Ok(())
    }

    /// Stop playback immediately (drops buffered audio).
    pub fn stop(&self) {
        let _ = self.pcm.drop();
    }

    /// Return the number of bytes currently writable without blocking.
    /// Recovers from XRUN instead of failing fatally.
    pub fn get_writable_bytes(&self) -> Result<usize> {
        match self.pcm.avail_update() {
            Ok(frames) => Ok(frames as usize * self.frame_bytes),
            Err(e) => {
                self.pcm
                    .try_recover(e, false)
                    .map_err(|e2| anyhow!("ALSA avail_update recovery failed: {}", e2))?;
                // After recovery, re-query
                let frames = self
                    .pcm
                    .avail_update()
                    .map_err(|e| anyhow!("ALSA avail_update failed after recovery: {}", e))?;
                Ok(frames as usize * self.frame_bytes)
            }
        }
    }

    /// Block until the device is ready to accept more data or `timeout_ms` elapses.
    /// Returns `true` if the device became ready, `false` on timeout.
    pub fn wait(&self, timeout_ms: u32) -> Result<bool> {
        self.pcm
            .wait(Some(timeout_ms))
            .map_err(|e| anyhow!("ALSA wait failed: {}", e))
    }

    pub fn period_bytes(&self) -> usize {
        self.period_bytes
    }

    pub fn buffer_bytes(&self) -> usize {
        self.buffer_bytes
    }
}

/// Probe the capabilities of a named ALSA device.
/// For USB audio devices, reads the authoritative USB stream descriptors from
/// /proc/asound/. Falls back to hw_params probing for non-USB devices.
pub fn probe_capabilities(device_name: &str) -> Result<(Vec<SampleRate>, Vec<BitsPerSample>)> {
    // Extract card number from "hw:N,D"
    if let Some(card_num) = parse_card_number(device_name) {
        if let Some(caps) = probe_usb_stream_descriptors(card_num) {
            return Ok(caps);
        }
    }
    probe_capabilities_hwparams(device_name)
}

/// Parse card number from an ALSA device name like "hw:5,0" → Some(5)
fn parse_card_number(device_name: &str) -> Option<u32> {
    let rest = device_name.strip_prefix("hw:")?;
    let card_str = rest.split(',').next()?;
    card_str.parse().ok()
}

/// Parse "hw:N,M" → Some((N, M)). Returns None for any non-raw-hw form
/// (default, plughw:, dmix:, etc.) — those don't map to a single /dev node.
fn parse_hw_card_dev(device_name: &str) -> Option<(u32, u32)> {
    let rest = device_name.strip_prefix("hw:")?;
    let mut parts = rest.split(',');
    let card: u32 = parts.next()?.parse().ok()?;
    let dev: u32 = parts.next()?.parse().ok()?;
    Some((card, dev))
}

/// Read USB audio stream descriptors from /proc/asound/cardN/stream0.
/// Returns None if the file doesn't exist (non-USB device).
fn probe_usb_stream_descriptors(card: u32) -> Option<(Vec<SampleRate>, Vec<BitsPerSample>)> {
    let path = format!("/proc/asound/card{}/stream0", card);
    let content = std::fs::read_to_string(&path).ok()?;

    // Find the Playback section
    let playback_section = content.split("Playback:").nth(1)?;

    let mut rates = Vec::new();
    let mut bits = Vec::new();

    for line in playback_section.lines() {
        let trimmed = line.trim();

        // Parse "Rates: 96000, 88200, 48000, 44100"
        if let Some(rates_str) = trimmed.strip_prefix("Rates:") {
            for rate_str in rates_str.split(',') {
                if let Ok(rate) = rate_str.trim().parse::<u32>() {
                    let sr = SampleRate(rate);
                    if !rates.contains(&sr) {
                        rates.push(sr);
                    }
                }
            }
        }

        // Parse "Bits: 24"
        if let Some(bits_str) = trimmed.strip_prefix("Bits:") {
            if let Ok(b) = bits_str.trim().parse::<u16>() {
                let bps = BitsPerSample(b);
                if !bits.contains(&bps) {
                    bits.push(bps);
                }
            }
        }
    }

    if rates.is_empty() || bits.is_empty() {
        return None;
    }

    rates.sort();
    bits.sort();
    Some((rates, bits))
}

/// Probe capabilities via ALSA hw_params (non-USB fallback).
/// Each test sets access, channels, format, AND rate together — some drivers
/// reject params unless the full configuration is constrained.
fn probe_capabilities_hwparams(device_name: &str) -> Result<(Vec<SampleRate>, Vec<BitsPerSample>)> {
    let pcm = PCM::new(device_name, Direction::Playback, false)
        .map_err(|e| anyhow!("Cannot open device '{}' for probing: {}", device_name, e))?;

    let all = Capabilities::all_possible();
    let mut supported_rates = Vec::new();
    let mut supported_bits = Vec::new();

    for &bits in &all.bits_per_samples {
        let Ok(fmt) = bits_to_format(bits) else {
            continue;
        };
        for &rate in &all.sample_rates {
            let Ok(hwp) = HwParams::any(&pcm) else {
                continue;
            };
            if hwp.set_access(Access::RWInterleaved).is_err() {
                continue;
            }
            if hwp.set_channels(2).is_err() {
                continue;
            }
            if hwp.set_format(fmt).is_err() {
                continue;
            }
            if hwp.set_rate(rate.0, ValueOr::Nearest).is_err() {
                continue;
            }
            let Ok(actual_rate) = hwp.get_rate() else {
                continue;
            };
            if actual_rate != rate.0 {
                continue;
            }
            if !supported_bits.contains(&bits) {
                supported_bits.push(bits);
            }
            if !supported_rates.contains(&rate) {
                supported_rates.push(rate);
            }
        }
    }

    Ok((supported_rates, supported_bits))
}

/// Attempt to elevate the current thread to real-time scheduling.
/// Falls back to `nice(-11)` if SCHED_FIFO is unavailable.
pub fn set_thread_priority(high_priority_mode: bool) {
    if !high_priority_mode {
        return;
    }

    let param = libc::sched_param { sched_priority: 50 };
    let result = unsafe { libc::sched_setscheduler(0, libc::SCHED_FIFO, &param) };
    if result != 0 {
        // nice() can return -1 on success, so check errno instead.
        unsafe { *libc::__errno_location() = 0 };
        unsafe { libc::nice(-11) };
        let errno = unsafe { *libc::__errno_location() };
        if errno != 0 {
            warn!("Failed to set thread priority: both SCHED_FIFO and nice() failed (errno={})", errno);
        }
    }
}
