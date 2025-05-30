use anyhow::Result;
use tokio::sync::mpsc::{channel, Sender};
use windows::Win32::{
    Devices::FunctionDiscovery::PKEY_DeviceInterface_FriendlyName,
    Media::Audio::IMMDevice,
    System::Com::{StructuredStorage::PropVariantToStringAlloc, STGM_READ},
};

use super::api::{com_initialize, AudioClient, ShareMode, ThreadPriority, WaveFormat};
use crate::audio::{Capabilities, DeviceTrait, StreamParams, StreamingData};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

pub struct Device {
    default_device_id: String,
    inner_device: IMMDevice,
    stream_thread_handle: Option<tokio::task::JoinHandle<Result<()>>>,
    high_priority_mode: bool,
    is_paused: Arc<AtomicBool>,
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

    fn start(&mut self, params: &StreamParams) -> Result<Sender<StreamingData>> {
        self.stop()?;
        let buffer = params.channels as usize
            * ((params.bits_per_sample as usize * params.samplerate as usize) / 8 as usize);
        let (data_tx, mut data_rx) = channel::<StreamingData>(buffer);

        let mut client = self.get_client(params)?;
        client.initialize()?;
        let high_priority_mode = self.high_priority_mode;
        let is_paused = self.is_paused.clone();

        self.stream_thread_handle = Some(tokio::spawn(async move {
            let _thread_priority = ThreadPriority::new(high_priority_mode)?;
            let mut client_started = false;
            let mut buffer = vec![];
            let mut available_buffer_size = client.get_available_buffer_size()?;
            while let Some(streaming_data) = data_rx.recv().await {
                if is_paused.load(Ordering::Relaxed) {
                    client.stop()?;
                    while is_paused.load(Ordering::Relaxed) {
                        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
                    }
                    client.start()?;
                }
                match streaming_data {
                    StreamingData::Data(data) => {
                        buffer.push(data);
                        if buffer.len() == available_buffer_size {
                            client.write(buffer.as_slice())?;
                            if !client_started {
                                client.start()?;
                                client_started = true;
                            }
                            client.wait_for_buffer()?;
                            available_buffer_size = client.get_available_buffer_size()?;
                            buffer.clear();
                        }
                    }
                    StreamingData::EndOfStream => break,
                };
            }
            client.stop()
        }));
        Ok(data_tx)
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
        if let Some(handle) = self.stream_thread_handle.take() {
            handle.abort();
        }
        Ok(())
    }
}
