use anyhow::Result;
use log::error;
use ringbuf::HeapRb;
use ringbuf::traits::{Consumer, Observer, Split};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use super::api::{AlsaPcm, set_thread_priority, probe_capabilities};
use crate::audio::{BufferConfig, Capabilities, DeviceTrait, StreamParams};
use crate::audio::device::{AudioPipeline, BufferSignal};

struct AlsaStreamHandle {
    thread: Option<std::thread::JoinHandle<Result<()>>>,
    is_playing: Arc<AtomicBool>,
    is_paused: Arc<AtomicBool>,
}

impl AlsaStreamHandle {
    fn pause(&self) {
        self.is_paused.store(true, Ordering::Release);
    }

    fn resume(&self) {
        self.is_paused.store(false, Ordering::Release);
    }

    fn stop(&mut self) {
        self.is_playing.store(false, Ordering::Release);
        self.is_paused.store(false, Ordering::Release);
        if let Some(handle) = self.thread.take() {
            match handle.join() {
                Ok(Err(e)) => error!("Audio-out thread error: {:#}", e),
                Err(_) => error!("Audio-out thread panicked"),
                Ok(Ok(())) => {}
            }
        }
    }
}

pub struct Device {
    device_name: String,
    friendly_name: String,
    is_default: bool,
    stream_handle: Option<AlsaStreamHandle>,
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

        let pcm = AlsaPcm::open(&self.device_name, params, buffer)?;
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

        let thread = Some(
            std::thread::Builder::new()
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
                })?,
        );

        self.stream_handle = Some(AlsaStreamHandle {
            thread,
            is_playing,
            is_paused,
        });

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
