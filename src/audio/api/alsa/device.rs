#[cfg(target_os = "linux")]
use std::sync::Arc;
#[cfg(target_os = "linux")]
use std::sync::atomic::{AtomicBool, Ordering};
#[cfg(target_os = "linux")]
use tokio::sync::{mpsc, Mutex};
#[cfg(target_os = "linux")]
use anyhow::Result;
#[cfg(target_os = "linux")]
use crate::audio::{Capabilities, StreamParams, StreamingData, DeviceTrait, BitsPerSample, SampleRate};
#[cfg(target_os = "linux")]
use crate::logging::log_to_file_only;

#[cfg(target_os = "linux")]
use super::alsa_api::AlsaPcm;

#[cfg(target_os = "linux")]
pub struct Device {
    name: String,
    is_default: bool,
    pcm: Arc<Mutex<Option<AlsaPcm>>>,
    is_playing: Arc<AtomicBool>,
    is_paused: Arc<AtomicBool>,
}

#[cfg(target_os = "linux")]
impl Device {
    pub fn new(name: String, is_default: bool) -> Self {
        Self {
            name,
            is_default,
            pcm: Arc::new(Mutex::new(None)),
            is_playing: Arc::new(AtomicBool::new(false)),
            is_paused: Arc::new(AtomicBool::new(false)),
        }
    }

    async fn get_alsa_format_from_bits_per_sample(bits: BitsPerSample) -> Result<alsa::pcm::Format> {
        match bits {
            BitsPerSample::Bits16 => Ok(alsa::pcm::Format::S16LE),
            BitsPerSample::Bits24 => Ok(alsa::pcm::Format::S243LE),
            BitsPerSample::Bits32 => Ok(alsa::pcm::Format::S32LE),
        }
    }

    async fn adjust_stream_params_for_capabilities(&self, params: &mut StreamParams, capabilities: &Capabilities) -> Result<()> {
        // Check if sample rate is supported
        if !capabilities.sample_rates.contains(&params.samplerate) {
            // Fall back to 44.1kHz if available
            if capabilities.sample_rates.contains(&SampleRate::Rate44100Hz) {
                params.samplerate = SampleRate::Rate44100Hz;
            } else {
                // Use the first available sample rate
                params.samplerate = capabilities.sample_rates[0];
            }
        }

        // Check if bits per sample is supported
        if !capabilities.bits_per_samples.contains(&params.bits_per_sample) {
            // Fall back to 16-bit if available
            if capabilities.bits_per_samples.contains(&BitsPerSample::Bits16) {
                params.bits_per_sample = BitsPerSample::Bits16;
            } else {
                // Use the first available bits per sample
                params.bits_per_sample = capabilities.bits_per_samples[0];
            }
        }

        Ok(())
    }
}

#[cfg(target_os = "linux")]
impl DeviceTrait for Device {
    fn is_default(&self) -> Result<bool> {
        Ok(self.is_default)
    }

    fn name(&self) -> Result<String> {
        Ok(self.name.clone())
    }

    fn get_capabilities(&self) -> Result<Capabilities> {
        // For ALSA, we'll return common capabilities
        // In a real implementation, we would query the device
        Ok(Capabilities {
            sample_rates: vec![
                SampleRate::Rate44100Hz,
                SampleRate::Rate48000Hz,
                SampleRate::Rate88200Hz,
                SampleRate::Rate96000Hz,
                SampleRate::Rate176400Hz,
                SampleRate::Rate192000Hz,
            ],
            bits_per_samples: vec![
                BitsPerSample::Bits16,
                BitsPerSample::Bits24,
                BitsPerSample::Bits32,
            ],
        })
    }

    fn start(&mut self, params: &StreamParams) -> Result<mpsc::Sender<StreamingData>> {
        let (tx, mut rx) = mpsc::channel::<StreamingData>(1024);

        // Adjust stream params based on device capabilities
        let _capabilities = self.get_capabilities()?;
        let adjusted_params = *params;

        // In async context, we need to handle the adjustment differently
        // For now, we'll just use the original params
        let stream_params = adjusted_params;

        let device_name = self.name.clone();
        let pcm_ref = self.pcm.clone();
        let is_playing_ref = self.is_playing.clone();
        let _is_paused_ref = self.is_paused.clone();

        // Log device initialization
        log_to_file_only("ALSA", &format!("Initializing device: {} (channels: {}, rate: {}, bits: {})",
                device_name, stream_params.channels,
                stream_params.samplerate as u32,
                stream_params.bits_per_sample as u32));

        // Spawn the audio streaming task
        tokio::spawn(async move {
            // Open the ALSA PCM device
            let alsa_pcm = match AlsaPcm::open(
                &device_name,
                stream_params.samplerate,
                stream_params.channels,
                stream_params.bits_per_sample,
                stream_params.exclusive,
            ) {
                Ok(pcm) => pcm,
                Err(e) => {
                    log_to_file_only("ALSA", &format!("ERROR Failed to open device {}: {}", device_name, e));
                    return;
                }
            };

            // Store the PCM in the device
            {
                let mut pcm_guard = pcm_ref.lock().await;
                *pcm_guard = Some(alsa_pcm);
            }

            is_playing_ref.store(true, Ordering::Relaxed);

            // Buffer for collecting bytes before writing to ALSA
            let mut write_buffer = Vec::new();
            let buffer_size = 4096; // Write in 4KB chunks

            // Process audio data from the channel
            while let Some(streaming_data) = rx.recv().await {
                match streaming_data {
                    StreamingData::Data(data) => {
                        write_buffer.push(data);

                        // Log every 1000 bytes received
                        if write_buffer.len() % 1000 == 0 {
                            log_to_file_only("ALSA", &format!("DEBUG Buffered {} bytes, waiting for {}", write_buffer.len(), buffer_size));
                        }

                        // Write buffer when it's full
                        if write_buffer.len() >= buffer_size {
                            let mut pcm_guard = pcm_ref.lock().await;
                            if let Some(ref mut pcm) = *pcm_guard {
                                if let Err(e) = pcm.write_bytes(&write_buffer) {
                                    log_to_file_only("ALSA", &format!("ERROR Writing audio data ({} bytes): {}", write_buffer.len(), e));
                                    break;
                                } else {
                                    // Debug: print first few bytes written
                                    log_to_file_only("ALSA", &format!("DEBUG Writing {} bytes successfully", write_buffer.len()));
                                    if write_buffer.len() >= 4 {
                                        let first_bytes: Vec<u8> = write_buffer.iter().take(4).cloned().collect();
                                        log_to_file_only("ALSA", &format!("DEBUG First 4 bytes: {:?}", first_bytes));
                                    }
                                }
                            }
                            write_buffer.clear();
                        }
                    }
                    StreamingData::EndOfStream => {
                        // Write any remaining data
                        if !write_buffer.is_empty() {
                            let mut pcm_guard = pcm_ref.lock().await;
                            if let Some(ref mut pcm) = *pcm_guard {
                                if let Err(e) = pcm.write_bytes(&write_buffer) {
                                    eprintln!("ALSA Error writing final audio data ({} bytes): {}", write_buffer.len(), e);
                                }
                            }
                            write_buffer.clear();
                        }

                        // Drain the audio buffer
                        let mut pcm_guard = pcm_ref.lock().await;
                        if let Some(ref mut pcm) = *pcm_guard {
                            log_to_file_only("ALSA", "Draining audio buffer");
                            let _ = pcm.drain();
                        }
                        log_to_file_only("ALSA", "End of stream processed");
                        break;
                    }
                }
            }

            is_playing_ref.store(false, Ordering::Relaxed);

            // Clean up the PCM device
            {
                let mut pcm_guard = pcm_ref.lock().await;
                *pcm_guard = None;
            }
        });

        Ok(tx)
    }

    fn pause(&mut self) -> Result<()> {
        let is_paused_ref = self.is_paused.clone();
        let pcm_ref = self.pcm.clone();

        tokio::spawn(async move {
            is_paused_ref.store(true, Ordering::Relaxed);
            let mut pcm_guard = pcm_ref.lock().await;
            if let Some(ref mut pcm) = *pcm_guard {
                let _ = pcm.pause();
            }
        });

        Ok(())
    }

    fn resume(&mut self) -> Result<()> {
        let is_paused_ref = self.is_paused.clone();
        let pcm_ref = self.pcm.clone();

        tokio::spawn(async move {
            is_paused_ref.store(false, Ordering::Relaxed);
            let mut pcm_guard = pcm_ref.lock().await;
            if let Some(ref mut pcm) = *pcm_guard {
                let _ = pcm.resume();
            }
        });

        Ok(())
    }

    fn stop(&mut self) -> Result<()> {
        let is_playing_ref = self.is_playing.clone();
        let pcm_ref = self.pcm.clone();

        tokio::spawn(async move {
            is_playing_ref.store(false, Ordering::Relaxed);
            let mut pcm_guard = pcm_ref.lock().await;
            if let Some(ref mut pcm) = *pcm_guard {
                let _ = pcm.drain();
            }
            *pcm_guard = None;
        });

        Ok(())
    }
}