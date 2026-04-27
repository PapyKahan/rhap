use anyhow::Result;
use ringbuf::HeapRb;
use ringbuf::traits::{Consumer, Observer, Split};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use super::api::{AlsaInitError, AlsaPcm, set_thread_priority, probe_capabilities};
use crate::audio::{BufferConfig, Capabilities, DeviceTrait, StreamParams};
use crate::audio::device::{AudioPipeline, BufferSignal};
use crate::audio::retry::{open_with_retry, RetryDecision, DEFAULT_INIT_BACKOFFS_MS};
use crate::audio::stream_handle::PullStreamHandle;

pub struct Device {
    device_name: String,
    friendly_name: String,
    is_default: bool,
    stream_handle: Option<PullStreamHandle>,
    high_priority_mode: bool,
}

impl Device {
    pub(crate) fn new(
        device_name: String,
        friendly_name: String,
        is_default: bool,
        high_priority_mode: bool,
    ) -> Self {
        Self {
            device_name,
            friendly_name,
            is_default,
            stream_handle: None,
            high_priority_mode,
        }
    }
}

impl DeviceTrait for Device {
    fn is_default(&self) -> Result<bool> {
        Ok(self.is_default)
    }

    fn name(&self) -> Result<String> {
        Ok(self.friendly_name.clone())
    }

    fn get_capabilities(&self) -> Result<Capabilities> {
        let (sample_rates, bits_per_samples) = probe_capabilities(&self.device_name)
            .unwrap_or_else(|e| {
                log::warn!("Capability probe failed for {}: {}", self.device_name, e);
                let all = Capabilities::all_possible();
                (all.sample_rates, all.bits_per_samples)
            });
        Ok(Capabilities {
            sample_rates,
            bits_per_samples,
        })
    }

    fn start(&mut self, params: &StreamParams, buffer: &BufferConfig) -> Result<AudioPipeline> {
        self.stop()?;

        let pcm = open_pcm_with_retry(&self.device_name, params, buffer)?;
        let alsa_buffer_bytes = pcm.buffer_bytes();

        let ring_bytes = buffer.ring_bytes_for(params).max(alsa_buffer_bytes * 4);
        let ring = HeapRb::<u8>::new(ring_bytes);
        let (producer, mut consumer) = ring.split();

        let end_of_stream = Arc::new(AtomicBool::new(false));
        let eos_clone = Arc::clone(&end_of_stream);

        let signal = Arc::new(BufferSignal::new());
        let signal_clone = Arc::clone(&signal);

        let is_playing = Arc::new(AtomicBool::new(true));
        let is_playing_clone = Arc::clone(&is_playing);
        let is_paused = Arc::new(AtomicBool::new(false));
        let is_paused_clone = Arc::clone(&is_paused);
        let high_priority_mode = self.high_priority_mode;

        let thread = std::thread::Builder::new()
            .name("rhap-audio-out".into())
            .spawn(move || -> Result<()> {
                    set_thread_priority(high_priority_mode);
                    let period_bytes = pcm.period_bytes();
                    let mut write_buf = vec![0u8; period_bytes];
                    let mut hw_paused = false;

                    loop {
                        if !is_playing_clone.load(Ordering::Acquire) {
                            break;
                        }

                        let want_paused = is_paused_clone.load(Ordering::Acquire);
                        if want_paused && !hw_paused {
                            pcm.pause()?;
                            hw_paused = true;
                        }
                        if want_paused {
                            std::thread::sleep(Duration::from_millis(10));
                            continue;
                        }
                        if hw_paused {
                            pcm.resume()?;
                            hw_paused = false;
                        }

                        let writable = pcm.get_writable_bytes()?;
                        let available = consumer.occupied_len();

                        if writable >= period_bytes && available >= period_bytes {
                            let n = consumer.pop_slice(&mut write_buf);
                            if n > 0 {
                                signal_clone.notify();
                                pcm.write(&write_buf[..n])?;
                            }
                            let _ = pcm.wait(100);
                        } else if eos_clone.load(Ordering::Acquire) {
                            // Drain remaining data using the pre-allocated buffer.
                            loop {
                                let remaining = consumer.occupied_len();
                                if remaining == 0 {
                                    break;
                                }
                                let n = consumer.pop_slice(&mut write_buf);
                                if n > 0 {
                                    signal_clone.notify();
                                    pcm.write(&write_buf[..n])?;
                                }
                            }
                            // Pad with silence to flush the last hardware period.
                            write_buf.fill(0);
                            pcm.write(&write_buf)?;
                            pcm.drain()?;
                            break;
                        } else if writable == 0 {
                            let _ = pcm.wait(20);
                        } else {
                            signal_clone.wait_timeout(Duration::from_millis(5));
                        }
                    }

                    pcm.stop();
                    Ok(())
                })?;

        self.stream_handle = Some(PullStreamHandle::new(thread, is_playing, is_paused));

        Ok(AudioPipeline {
            producer,
            end_of_stream,
            signal,
        })
    }

    fn pause(&mut self) -> Result<()> {
        if let Some(handle) = &self.stream_handle {
            handle.pause();
        }
        Ok(())
    }

    fn resume(&mut self) -> Result<()> {
        if let Some(handle) = &self.stream_handle {
            handle.resume();
        }
        Ok(())
    }

    fn stop(&mut self) -> Result<()> {
        if let Some(mut handle) = self.stream_handle.take() {
            handle.stop();
        }
        Ok(())
    }
}

/// Open a PCM device using the generic retry helper. Each attempt fully
/// reopens the PCM (close+open) so a previous EBUSY holder has time to
/// release its handle.
fn open_pcm_with_retry(
    device_name: &str,
    params: &StreamParams,
    buffer: &BufferConfig,
) -> Result<AlsaPcm> {
    open_with_retry("alsa: open", DEFAULT_INIT_BACKOFFS_MS, || {
        match AlsaPcm::open_classified(device_name, params, buffer) {
            Ok(pcm) => RetryDecision::Ok(pcm),
            Err(AlsaInitError::Busy) => RetryDecision::BackoffRetry,
            Err(AlsaInitError::Permanent(e)) => RetryDecision::Fatal(e),
        }
    })
}
