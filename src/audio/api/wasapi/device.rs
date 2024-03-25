use anyhow::Result;
use tokio::sync::mpsc::{Sender, channel};
use windows::Win32::{Media::Audio::{IMMDevice, IAudioClient}, System::Com::{STGM_READ, StructuredStorage::PropVariantToStringAlloc, CLSCTX_ALL}, Devices::FunctionDiscovery::PKEY_DeviceInterface_FriendlyName};

use super::{api::{com_initialize, ShareMode, AudioClient, WaveFormat}, stream::Streamer};
use crate::audio::{Capabilities, DeviceTrait, StreamParams, StreamingCommand, StreamingData};

pub struct Device {
    default_device_id: String,
    inner_device: IMMDevice,
    stream_thread_handle: Option<tokio::task::JoinHandle<Result<()>>>,
    command: Option<Sender<StreamingCommand>>,
}

impl Device {
    pub(super) fn new(inner_device: IMMDevice, default_device_id: String) -> Result<Self> {
        Ok(Self {
            inner_device,
            default_device_id,
            stream_thread_handle: Option::None,
            command: Option::None,
        })
    }

    pub(super) fn get_id(&self) -> Result<String> {
        Ok(unsafe { self.inner_device.GetId()?.to_string()? })
    }

    pub fn get_client(&self) -> Result<AudioClient> {
        AudioClient::new(unsafe {self.inner_device.Activate::<IAudioClient>(CLSCTX_ALL, None)? })
    }

    fn capabilities(&self) -> Result<Capabilities> {
        let mut sample_rates = Vec::new();
        let mut bits_per_samples = Vec::new();

        let default_capabilities = Capabilities::default();

        com_initialize();
        for bits_per_sample in default_capabilities.bits_per_samples {
            let default_capabilities = Capabilities::default();
            for samplerate in default_capabilities.sample_rates {
                let client = self.get_client()?;
                let wave_format = WaveFormat::new(
                    bits_per_sample,
                    samplerate as usize,
                    2
                );
                let sharemode = match true {
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

    fn start(&mut self, params: StreamParams) -> Result<Sender<StreamingData>> {
        self.stop()?;
        let (command_tx, _) = channel::<StreamingCommand>(32);
        self.command = Some(command_tx);
        let buffer = params.channels as usize * ((params.bits_per_sample as usize * params.samplerate as usize) / 8 as usize);
        let (data_tx, data_rx) = channel::<StreamingData>(buffer);
        let mut streamer = Streamer::new(&self, data_rx, params)?;
        self.stream_thread_handle = Some(tokio::spawn(async move { streamer.start().await }));
        Ok(data_tx)
    }

    fn pause(&mut self) -> Result<()> {
        if let Some(command) = self.command.take() {
            command.blocking_send(StreamingCommand::Pause)?;
        }
        Ok(())
    }

    fn resume(&mut self) -> Result<()> {
        if let Some(command) = self.command.take() {
            command.blocking_send(StreamingCommand::Resume)?;
        }
        Ok(())
    }

    fn stop(&mut self) -> Result<()> {
        if let Some(command) = self.command.take() {
            drop(command);
        }
        if let Some(handle) = self.stream_thread_handle.take() {
            handle.abort_handle().abort();
        }
        Ok(())
    }
}
