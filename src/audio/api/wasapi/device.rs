use std::sync::mpsc::{SyncSender, sync_channel};

use anyhow::{anyhow, Result};
use wasapi::{ShareMode, WaveFormat};

use super::{com::com_initialize, stream::Streamer};
use crate::audio::{Capabilities, DeviceTrait, StreamParams, StreamingCommand};

pub struct Device {
    is_default: bool,
    inner_device: wasapi::Device,
    stream_thread_handle: Option<tokio::task::JoinHandle<Result<()>>>,
    command: Option<SyncSender<StreamingCommand>>,
}

impl Device {
    pub(super) fn new(inner_device: wasapi::Device, is_default: bool) -> Result<Self> {
        Ok(Self {
            inner_device,
            is_default,
            stream_thread_handle: Option::None,
            command: Option::None,
        })
    }

    fn capabilities(device: &wasapi::Device) -> Result<Capabilities> {
        let mut sample_rates = Vec::new();
        let mut bits_per_samples = Vec::new();

        let default_capabilities = Capabilities::default();

        com_initialize();
        for bits_per_sample in default_capabilities.bits_per_samples {
            let sample_type = match bits_per_sample {
                crate::audio::BitsPerSample::Bits8 => &wasapi::SampleType::Int,
                crate::audio::BitsPerSample::Bits16 => &wasapi::SampleType::Int,
                crate::audio::BitsPerSample::Bits24 => &wasapi::SampleType::Int,
                crate::audio::BitsPerSample::Bits32 => &wasapi::SampleType::Float,
            };
            let default_capabilities = Capabilities::default();
            for samplerate in default_capabilities.sample_rates {
                let client = device
                    .get_iaudioclient()
                    .map_err(|e| anyhow!("IAudioClient::GetAudioClient failed: {}", e))?;
                let wave_format = WaveFormat::new(
                    bits_per_sample as usize,
                    bits_per_sample as usize,
                    sample_type,
                    samplerate as usize,
                    2,
                    None,
                );
                let sharemode = match true {
                    true => ShareMode::Exclusive,
                    false => ShareMode::Shared,
                };
                match sharemode {
                    ShareMode::Exclusive => {
                        if let Ok(_) = client.is_supported_exclusive_with_quirks(&wave_format) {
                            if !bits_per_samples.contains(&bits_per_sample) {
                                bits_per_samples.push(bits_per_sample);
                            };
                            if !sample_rates.contains(&samplerate) {
                                sample_rates.push(samplerate);
                            };
                        }
                    }
                    ShareMode::Shared => match client.is_supported(&wave_format, &sharemode) {
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
    fn is_default(&self) -> bool {
        self.is_default
    }

    fn name(&self) -> String {
        self.inner_device.get_friendlyname().unwrap_or_default()
    }

    fn get_capabilities(&self) -> Result<Capabilities> {
        Device::capabilities(&self.inner_device)
    }

    fn start(&mut self, params: StreamParams) -> Result<SyncSender<u8>> {
        let (command_tx, command_rx) = sync_channel::<StreamingCommand>(32);
        self.command = Some(command_tx);
        let buffer = params.channels as usize * ((params.bits_per_sample as usize * params.samplerate as usize) / 8 as usize);
        println!("buffer: {}", buffer);
        let (data_tx, data_rx) = sync_channel::<u8>(buffer);
        let mut streamer = Streamer::new(&self.inner_device, data_rx, command_rx, params)?;
        self.stream_thread_handle = Some(tokio::spawn(async move { streamer.start() }));
        Ok(data_tx)
    }

    fn pause(&mut self) -> Result<()> {
        if let Some(command) = self.command.take() {
            command.send(StreamingCommand::Pause)?;
        }
        Ok(())
    }

    fn resume(&mut self) -> Result<()> {
        if let Some(command) = self.command.take() {
            command.send(StreamingCommand::Resume)?;
        }
        Ok(())
    }

    fn stop(&mut self) -> Result<()> {
        if let Some(command) = self.command.take() {
            command.send(StreamingCommand::Stop)?;
            drop(command);
        }
        if let Some(handle) = self.stream_thread_handle.take() {
            handle.abort();
        }
        Ok(())
    }
}
