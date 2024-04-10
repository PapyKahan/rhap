use anyhow::Result;
use tokio::sync::mpsc::{channel, Sender};
use windows::Win32::{
    Devices::FunctionDiscovery::PKEY_DeviceInterface_FriendlyName,
    Media::Audio::IMMDevice,
    System::Com::{StructuredStorage::PropVariantToStringAlloc, STGM_READ},
};

use super::{
    api::{com_initialize, AudioClient, ShareMode, WaveFormat},
    stream::Streamer,
};
use crate::audio::{Capabilities, DeviceTrait, StreamParams, StreamingData};

pub struct Device {
    default_device_id: String,
    inner_device: IMMDevice,
    stream_thread_handle: Option<tokio::task::JoinHandle<Result<()>>>
}

impl StreamParams {
    fn create_wave_format(&self) -> WaveFormat {
        WaveFormat::new(self.bits_per_sample, self.samplerate as usize, self.channels as usize)
    }
}

impl Device {
    pub(crate) fn new(inner_device: IMMDevice, default_device_id: String) -> Result<Self> {
        Ok(Self {
            inner_device,
            default_device_id,
            stream_thread_handle: Option::None,
        })
    }

    pub(crate) fn get_id(&self) -> Result<String> {
        Ok(unsafe { self.inner_device.GetId()?.to_string()? })
    }

    pub fn get_client(&self, params: &StreamParams) -> Result<AudioClient> {
        AudioClient::new(&self.inner_device, params)
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
                    pollmode: false
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
        self.capabilities()
    }

    fn start(&mut self, params: &StreamParams) -> Result<Sender<StreamingData>> {
        self.stop()?;
        let buffer = params.channels as usize
            * ((params.bits_per_sample as usize * params.samplerate as usize) / 8 as usize);
        let (data_tx, data_rx) = channel::<StreamingData>(buffer);
        let mut streamer = Streamer::new(&self, data_rx, params)?;
        self.stream_thread_handle = Some(tokio::spawn(async move {
            let result = streamer.start().await;
            if let Some(error) = result.as_ref().err() {
                println!("Error: {:?}", error);
            }
            result
        }));
        Ok(data_tx)
    }

    fn pause(&mut self) -> Result<()> {
        Ok(())
    }

    fn resume(&mut self) -> Result<()> {
        Ok(())
    }

    fn stop(&mut self) -> Result<()> {
        if let Some(handle) = self.stream_thread_handle.take() {
            handle.abort_handle().abort();
        }
        Ok(())
    }
}
