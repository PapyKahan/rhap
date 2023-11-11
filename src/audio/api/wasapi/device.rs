use anyhow::{anyhow, Result};
use std::sync::mpsc::{sync_channel, Receiver, SyncSender};
use wasapi::{ShareMode, WaveFormat};

use super::{com::com_initialize, stream::Streamer};
use crate::audio::{Capabilities, DeviceTrait, StreamParams, StreamingCommand};

pub struct Device {
    pub is_default: bool,
    pub(super) inner_device: wasapi::Device,
    pub(super) receiver: Option<Receiver<StreamingCommand>>,
    pub(super) stream_thread_handle: Option<std::thread::JoinHandle<Result<()>>>,
}

impl Device {
    pub(super) fn new(inner_device: wasapi::Device, is_default: bool) -> Self {
        Self {
            inner_device,
            receiver: Option::None,
            is_default,
            stream_thread_handle: Option::None,
        }
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
                let client = self
                    .inner_device
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

        wasapi::deinitialize();

        Ok(crate::audio::Capabilities {
            sample_rates,
            bits_per_samples,
        })
    }

    fn start(&mut self, params: StreamParams) -> Result<SyncSender<StreamingCommand>> {
        let (tx, rx) = sync_channel::<StreamingCommand>(16384);
        let mut streamer = Streamer::new(&self, rx, params)?;
        self.stream_thread_handle = Some(std::thread::spawn(move || -> Result<()> {
            streamer.start()?;
            return Ok(());
        }));
        Ok(tx)
    }

    fn stop(&mut self) -> Result<()> {
        if let Some(receiver) = self.receiver.take() {
            drop(receiver);
        }
        if let Some(handle) = self.stream_thread_handle.take() {
            handle
                .join()
                .unwrap_or_else(|_| Ok(()))
                .map_err(|err| anyhow!(err.to_string()))?;
        }
        Ok(())
    }
}
