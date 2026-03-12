use anyhow::Result;
use ringbuf::HeapRb;
use ringbuf::traits::{Consumer, Observer, Split};
use windows::Win32::{
    Devices::FunctionDiscovery::PKEY_DeviceInterface_FriendlyName,
    Media::Audio::IMMDevice,
    System::Com::{StructuredStorage::PropVariantToStringAlloc, STGM_READ},
};

use super::api::{com_initialize, AudioClient, ShareMode, ThreadPriority, WaveFormat};
use crate::audio::{Capabilities, DeviceTrait, StreamParams};
use crate::audio::device::AudioPipeline;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

pub struct Device {
    default_device_id: String,
    inner_device: IMMDevice,
    stream_thread_handle: Option<std::thread::JoinHandle<Result<()>>>,
    high_priority_mode: bool,
    is_paused: Arc<AtomicBool>,
    is_playing: Arc<AtomicBool>,
}

impl StreamParams {
    fn create_wave_format(&self) -> WaveFormat {
        WaveFormat::new(
            self.bits_per_sample,
            self.samplerate as usize,
            self.channels as usize,
        )
    }
}

impl Device {
    pub(crate) fn new(
        inner_device: IMMDevice,
        default_device_id: String,
        high_priority_mode: bool,
    ) -> Result<Self> {
        Ok(Self {
            inner_device,
            default_device_id,
            stream_thread_handle: Option::None,
            high_priority_mode,
            is_paused: Arc::new(AtomicBool::new(false)),
            is_playing: Arc::new(AtomicBool::new(false)),
        })
    }

    pub(crate) fn get_id(&self) -> Result<String> {
        Ok(unsafe { self.inner_device.GetId()?.to_string()? })
    }

    pub fn get_client(&self, params: &StreamParams) -> Result<AudioClient> {
        AudioClient::new(&self.inner_device, params)
    }
}

unsafe impl Send for Device {}
unsafe impl Sync for Device {}

impl DeviceTrait for Device {
    fn is_default(&self) -> Result<bool> {
        Ok(self.default_device_id == self.get_id()?)
    }

    fn name(&self) -> Result<String> {
        let store = unsafe { self.inner_device.OpenPropertyStore(STGM_READ)? };
        let prop = unsafe { store.GetValue(&PKEY_DeviceInterface_FriendlyName)? };
        Ok(unsafe { PropVariantToStringAlloc(&prop)?.to_string()? })
    }

    fn get_capabilities(&self) -> Result<Capabilities> {
        let mut sample_rates = Vec::new();
        let mut bits_per_samples = Vec::new();

        let default_capabilities = Capabilities::default();

        com_initialize();
        for bits_per_sample in default_capabilities.bits_per_samples {
            let default_capabilities = Capabilities::default();
            for samplerate in default_capabilities.sample_rates {
                let params = StreamParams {
                    samplerate,
                    bits_per_sample,
                    channels: 2,
                    exclusive: true,
                    pollmode: false,
                };
                let client = self.get_client(&params)?;
                let wave_format = params.create_wave_format();
                let sharemode = match params.exclusive {
                    true => ShareMode::Exclusive,
                    false => ShareMode::Shared,
                };
                match sharemode {
                    ShareMode::Exclusive => {
                        if let Ok(_) = client.is_supported(wave_format, &sharemode) {
                            if !bits_per_samples.contains(&bits_per_sample) {
                                bits_per_samples.push(bits_per_sample);
                            };
                            if !sample_rates.contains(&samplerate) {
                                sample_rates.push(samplerate);
                            };
                        }
                    }
                    ShareMode::Shared => match client.is_supported(wave_format, &sharemode) {
                        Ok(_) => {
                            if !bits_per_samples.contains(&bits_per_sample) {
                                bits_per_samples.push(bits_per_sample);
                            };
                            if !sample_rates.contains(&samplerate) {
                                sample_rates.push(samplerate);
                            };
                        }
                        Err(_) => {}
                    },
                }
            }
        }

        Ok(crate::audio::Capabilities {
            sample_rates,
            bits_per_samples,
        })
    }

    fn start(&mut self, params: &StreamParams) -> Result<AudioPipeline> {
        self.stop()?;

        let mut client = self.get_client(params)?;
        client.initialize()?;
        let wasapi_buffer_bytes = client.get_available_buffer_size()?;

        let ring = HeapRb::<u8>::new(wasapi_buffer_bytes * 4);
        let (producer, mut consumer) = ring.split();

        let end_of_stream = Arc::new(AtomicBool::new(false));
        let eos_clone = Arc::clone(&end_of_stream);

        self.is_playing.store(true, Ordering::Release);
        let is_playing = Arc::clone(&self.is_playing);
        let is_paused = Arc::clone(&self.is_paused);
        let high_priority_mode = self.high_priority_mode;

        self.stream_thread_handle = Some(
            std::thread::Builder::new()
                .name("rhap-audio-out".into())
                .spawn(move || -> Result<()> {
                    com_initialize();
                    let _thread_priority = ThreadPriority::new(high_priority_mode)?;
                    let mut client_started = false;
                    let mut buffer = vec![0u8; wasapi_buffer_bytes];

                    loop {
                        if !is_playing.load(Ordering::Relaxed) {
                            break;
                        }

                        if is_paused.load(Ordering::Relaxed) {
                            client.stop()?;
                            while is_paused.load(Ordering::Relaxed) {
                                if !is_playing.load(Ordering::Relaxed) {
                                    return Ok(());
                                }
                                std::thread::sleep(Duration::from_millis(10));
                            }
                            client.start()?;
                        }

                        let available = consumer.occupied_len();

                        if available >= wasapi_buffer_bytes {
                            let n = consumer.pop_slice(&mut buffer);
                            if n > 0 {
                                client.write(&buffer[..n])?;
                                if !client_started {
                                    client.start()?;
                                    client_started = true;
                                }
                                client.wait_for_buffer()?;
                            }
                        } else if eos_clone.load(Ordering::Acquire) {
                            // Drain remaining data
                            let remaining = consumer.occupied_len();
                            if remaining > 0 {
                                let mut drain_buf = vec![0u8; remaining];
                                let n = consumer.pop_slice(&mut drain_buf);
                                if n > 0 {
                                    // Pad to full buffer size for WASAPI
                                    drain_buf.resize(wasapi_buffer_bytes, 0);
                                    client.write(&drain_buf)?;
                                    if !client_started {
                                        client.start()?;
                                        client_started = true;
                                    }
                                    client.wait_for_buffer()?;
                                }
                            }
                            break;
                        } else {
                            std::thread::sleep(Duration::from_micros(500));
                        }
                    }

                    client.stop()
                })?,
        );

        Ok(AudioPipeline {
            producer,
            end_of_stream,
            buffer_size_bytes: wasapi_buffer_bytes,
        })
    }

    fn pause(&mut self) -> Result<()> {
        self.is_paused.store(true, Ordering::Relaxed);
        Ok(())
    }

    fn resume(&mut self) -> Result<()> {
        self.is_paused.store(false, Ordering::Relaxed);
        Ok(())
    }

    fn stop(&mut self) -> Result<()> {
        self.is_playing.store(false, Ordering::Release);
        if let Some(handle) = self.stream_thread_handle.take() {
            let _ = handle.join();
        }
        Ok(())
    }
}
