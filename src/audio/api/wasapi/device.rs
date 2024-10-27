use anyhow::Result;
use async_trait::async_trait;
use windows::Win32::{
    Devices::FunctionDiscovery::PKEY_DeviceInterface_FriendlyName,
    Media::Audio::IMMDevice,
    System::Com::{StructuredStorage::PropVariantToStringAlloc, STGM_READ},
};

use super::api::{com_initialize, AudioClient, ShareMode, WaveFormat};
use crate::audio::{Capabilities, DeviceTrait, StreamParams};

pub struct Device {
    default_device_id: String,
    inner_device: IMMDevice,
    current_client: Option<AudioClient>,
    pub high_priority_mode: bool,
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
            current_client: None,
            default_device_id,
            high_priority_mode,
        })
    }

    pub(crate) fn get_client(&self, params: &StreamParams) -> Result<AudioClient> {
        AudioClient::new(&self.inner_device, params)
    }

    pub(crate) fn get_id(&self) -> Result<String> {
        Ok(unsafe { self.inner_device.GetId()?.to_string()? })
    }

    fn capabilities(&self) -> Result<Capabilities> {
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
}

unsafe impl Send for Device {}
unsafe impl Sync for Device {}

#[async_trait]
impl DeviceTrait for Device {
    fn is_default(&self) -> Result<bool> {
        Ok(self.default_device_id == self.get_id()?)
    }

    fn name(&self) -> Result<String> {
        let store = unsafe { self.inner_device.OpenPropertyStore(STGM_READ)? };
        let prop = unsafe { store.GetValue(&PKEY_DeviceInterface_FriendlyName)? };
        Ok(unsafe { PropVariantToStringAlloc(&prop)?.to_string()? })
    }

    fn start(&mut self, params: &StreamParams) -> Result<()> {
        let mut client = self.get_client(params)?;
        client.initialize()?;
        client.start()?;
        self.current_client = Some(client);
        Ok(())
    }

    fn get_capabilities(&self) -> Result<Capabilities> {
        self.capabilities()
    }

    async fn write(&mut self, data: &[u8]) -> Result<()> {
        if let Some(client) = &mut self.current_client {
            let writen = 0;
            loop {
                println!("waiting for buffer...");
                client.wait_for_buffer()?;
                let mut to_write = client.get_buffer_size() - client.get_current_padding_size()?;
                println!("got buffer, to write {}", to_write);
                if writen + to_write > data.len() {
                    to_write = data.len() - writen;
                    println!("to write {}", to_write);
                    client.write(&data[writen..writen + to_write])?;
                    println!("wrote {} bytes", to_write);
                    break;
                } else {
                    println!("to write {}", to_write);
                    client.write(&data[writen..writen + to_write])?;
                    println!("wrote {} bytes", to_write);
                }
            }
        }
        Ok(())
    }

    fn stop(&mut self) -> Result<()> {
        if let Some(client) = self.current_client.take() {
            client.stop()?;
        }
        Ok(())
    }
}
