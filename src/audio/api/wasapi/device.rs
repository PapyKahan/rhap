use anyhow::Result;
use log::error;
use ringbuf::HeapRb;
use ringbuf::traits::{Consumer, Observer, Split};
use windows::Win32::{
    Devices::FunctionDiscovery::PKEY_DeviceInterface_FriendlyName,
    Media::Audio::IMMDevice,
    System::Com::{StructuredStorage::PropVariantToStringAlloc, STGM_READ},
};

use super::api::{com_initialize, AudioClient, ShareMode, ThreadPriority, WaveFormat};
use crate::audio::{BufferConfig, Capabilities, DeviceTrait, StreamParams};
use crate::audio::device::{AudioPipeline, BufferSignal};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

struct WasapiStreamHandle {
    thread: Option<std::thread::JoinHandle<Result<()>>>,
    is_playing: Arc<AtomicBool>,
    is_paused: Arc<AtomicBool>,
}

impl WasapiStreamHandle {
    fn pause(&self) {
        self.is_paused.store(true, Ordering::Release);
    }

    fn resume(&self) {
        self.is_paused.store(false, Ordering::Release);
    }

    fn stop(&mut self) {
        self.is_playing.store(false, Ordering::Release);
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
    default_device_id: String,
    inner_device: IMMDevice,
    stream_handle: Option<WasapiStreamHandle>,
    high_priority_mode: bool,
}

impl StreamParams {
    fn create_wave_format(&self) -> WaveFormat {
        WaveFormat::new(
            self.bits_per_sample,
            self.samplerate,
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
            stream_handle: None,
            high_priority_mode,
        })
    }

    pub(crate) fn get_id(&self) -> Result<String> {
        Ok(unsafe { self.inner_device.GetId()?.to_string()? })
    }

    pub fn get_client(&self, params: &StreamParams) -> Result<AudioClient> {
        AudioClient::new(&self.inner_device, params)
    }
}

// SAFETY: IMMDevice is a COM pointer initialized in MTA (COINIT_MULTITHREADED).
// MTA COM objects can be safely called from any thread. Device is moved between
// threads but not shared concurrently.
unsafe impl Send for Device {}

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

        let all = Capabilities::all_possible();

        com_initialize();
        // Create one AudioClient — only inner_client is used for IsFormatSupported
        let dummy_params = StreamParams {
            samplerate: all.sample_rates[0],
            bits_per_sample: all.bits_per_samples[0],
            channels: 2,
            exclusive: true,
            pollmode: false,
        };
        let client = self.get_client(&dummy_params)?;
        let sharemode = ShareMode::Exclusive;

        for bits_per_sample in &all.bits_per_samples {
            for samplerate in &all.sample_rates {
                let params = StreamParams {
                    samplerate: *samplerate,
                    bits_per_sample: *bits_per_sample,
                    channels: 2,
                    exclusive: true,
                    pollmode: false,
                };
                let wave_format = params.create_wave_format();
                if client.is_supported(wave_format, &sharemode).is_ok() {
                    if !bits_per_samples.contains(bits_per_sample) {
                        bits_per_samples.push(*bits_per_sample);
                    }
                    if !sample_rates.contains(samplerate) {
                        sample_rates.push(*samplerate);
                    }
                }
            }
        }

        Ok(Capabilities {
            sample_rates,
            bits_per_samples,
        })
    }

    fn start(&mut self, params: &StreamParams, buffer: &BufferConfig) -> Result<AudioPipeline> {
        self.stop()?;

        let mut client = self.get_client(params)?;
        client.initialize(buffer)?;
        let wasapi_buffer_bytes = client.get_available_buffer_size()?;

        let ring_bytes = buffer.ring_bytes_for(params).max(wasapi_buffer_bytes * 4);
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
                    com_initialize();
                    let _thread_priority = ThreadPriority::new(high_priority_mode)?;
                    let mut client_started = false;
                    let mut buffer = vec![0u8; wasapi_buffer_bytes];

                    loop {
                        if !is_playing_clone.load(Ordering::Acquire) {
                            break;
                        }

                        if is_paused_clone.load(Ordering::Acquire) {
                            client.stop()?;
                            while is_paused_clone.load(Ordering::Acquire) {
                                if !is_playing_clone.load(Ordering::Acquire) {
                                    return Ok(());
                                }
                                std::thread::sleep(Duration::from_millis(10));
                            }
                            client.start()?;
                        }

                        // Query how much WASAPI can accept. In event mode this
                        // is always the full buffer; in poll mode it may be less
                        // (e.g. half on double-buffered devices).
                        let writable = client.get_writable_size()?;
                        let available = consumer.occupied_len();

                        if writable > 0 && available >= writable {
                            let n = consumer.pop_slice(&mut buffer[..writable]);
                            if n > 0 {
                                signal_clone.notify();
                                client.write(&buffer[..n])?;
                                if !client_started {
                                    client.start()?;
                                    client_started = true;
                                }
                                client.wait_for_buffer()?;
                            }
                        } else if eos_clone.load(Ordering::Acquire) {
                            // Drain remaining data, pad with silence
                            let remaining = consumer.occupied_len();
                            let chunk = writable.max(remaining);
                            if chunk > 0 {
                                let mut drain_buf = vec![0u8; chunk];
                                let n = consumer.pop_slice(&mut drain_buf);
                                if n > 0 {
                                    signal_clone.notify();
                                }
                                client.write(&drain_buf)?;
                                if !client_started {
                                    client.start()?;
                                }
                                client.wait_for_buffer()?;
                            }
                            break;
                        } else if writable == 0 {
                            std::thread::sleep(Duration::from_micros(100));
                        } else {
                            signal_clone.wait_timeout(Duration::from_millis(5));
                        }
                    }

                    client.stop()
                })?,
        );

        self.stream_handle = Some(WasapiStreamHandle {
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
