use anyhow::Result;
use log::error;
use ringbuf::HeapRb;
use ringbuf::traits::{Consumer, Observer, Split};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use super::api::{AlsaPcm, ThreadPriority, probe_capabilities};
use crate::audio::{Capabilities, DeviceTrait, StreamParams};
use crate::audio::device::{AudioPipeline, BufferSignal};

pub struct Device {
    device_name: String,
    friendly_name: String,
    is_default: bool,
    stream_thread_handle: Option<std::thread::JoinHandle<Result<()>>>,
    high_priority_mode: bool,
    is_playing: Arc<AtomicBool>,
    is_paused: Arc<AtomicBool>,
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
            stream_thread_handle: None,
            high_priority_mode,
            is_playing: Arc::new(AtomicBool::new(false)),
            is_paused: Arc::new(AtomicBool::new(false)),
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
            .unwrap_or_else(|_| {
                let all = Capabilities::all_possible();
                (all.sample_rates, all.bits_per_samples)
            });
        Ok(Capabilities {
            sample_rates,
            bits_per_samples,
        })
    }

    fn start(&mut self, params: &StreamParams) -> Result<AudioPipeline> {
        self.stop()?;

        let pcm = AlsaPcm::open(&self.device_name, params)?;
        let alsa_buffer_bytes = pcm.buffer_bytes();

        let ring = HeapRb::<u8>::new(alsa_buffer_bytes * 4);
        let (producer, mut consumer) = ring.split();

        let end_of_stream = Arc::new(AtomicBool::new(false));
        let eos_clone = Arc::clone(&end_of_stream);

        let signal = Arc::new(BufferSignal::new());
        let signal_clone = Arc::clone(&signal);

        self.is_playing.store(true, Ordering::Release);
        let is_playing = Arc::clone(&self.is_playing);
        let is_paused = Arc::clone(&self.is_paused);
        let high_priority_mode = self.high_priority_mode;

        self.stream_thread_handle = Some(
            std::thread::Builder::new()
                .name("rhap-audio-out".into())
                .spawn(move || -> Result<()> {
                    let _priority = ThreadPriority::new(high_priority_mode)?;
                    let period_bytes = pcm.period_bytes();
                    // Pre-allocate the write buffer once to avoid per-period heap allocation.
                    let mut write_buf = vec![0u8; period_bytes];

                    loop {
                        if !is_playing.load(Ordering::Acquire) {
                            break;
                        }

                        if is_paused.load(Ordering::Acquire) {
                            pcm.pause()?;
                            while is_paused.load(Ordering::Acquire) {
                                if !is_playing.load(Ordering::Acquire) {
                                    return Ok(());
                                }
                                std::thread::sleep(Duration::from_millis(10));
                            }
                            pcm.resume()?;
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
                            let remaining = consumer.occupied_len();
                            if remaining > 0 {
                                let mut drain_buf = vec![0u8; remaining];
                                let n = consumer.pop_slice(&mut drain_buf);
                                if n > 0 {
                                    signal_clone.notify();
                                    pcm.write(&drain_buf[..n])?;
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

        Ok(AudioPipeline {
            producer,
            end_of_stream,
            signal,
        })
    }

    fn pause(&mut self) -> Result<()> {
        self.is_paused.store(true, Ordering::Release);
        Ok(())
    }

    fn resume(&mut self) -> Result<()> {
        self.is_paused.store(false, Ordering::Release);
        Ok(())
    }

    fn stop(&mut self) -> Result<()> {
        self.is_playing.store(false, Ordering::Release);
        self.is_paused.store(false, Ordering::Release);
        if let Some(handle) = self.stream_thread_handle.take() {
            match handle.join() {
                Ok(Err(e)) => error!("Audio-out thread error: {:#}", e),
                Err(_) => error!("Audio-out thread panicked"),
                Ok(Ok(())) => {}
            }
        }
        Ok(())
    }
}
