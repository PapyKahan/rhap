use super::{Capabilities, StreamParams, StreamingData};
#[cfg(windows)]
use super::api;
#[cfg(unix)]
use super::api;
use anyhow::{anyhow, Result};
use tokio::sync::mpsc::Sender;

pub trait DeviceTrait: Send + Sync {
    fn is_default(&self) -> Result<bool>;
    fn name(&self) -> Result<String>;
    fn get_capabilities(&self) -> Result<Capabilities>;
    fn start(&mut self, params: &StreamParams) -> Result<Sender<StreamingData>>;
    fn pause(&mut self) -> Result<()>;
    fn resume(&mut self) -> Result<()>;
    fn stop(&mut self) -> Result<()>;
}

pub enum Device {
    None,
    #[cfg(windows)]
    Wasapi(api::wasapi::device::Device),
    #[cfg(unix)]
    Jack(api::jack::device::Device),
}

impl Device {
    pub fn adjust_stream_params(&self, params: &StreamParams) -> Result<StreamParams> {
        let capabilities = self.get_capabilities()?;
        let contains_sample_rates = capabilities.sample_rates.contains(&params.samplerate);
        let contains_bits_per_samples = capabilities.bits_per_samples.contains(&params.bits_per_sample);
        if !contains_sample_rates || !contains_bits_per_samples {
            let samplerate = if contains_sample_rates {
                params.samplerate
            } else {
                *capabilities.sample_rates.last().unwrap()
            };
            let bits_per_sample = if contains_bits_per_samples {
                params.bits_per_sample
            } else {
                *capabilities.bits_per_samples.last().unwrap()
            };
            return Ok(StreamParams {
                samplerate,
                bits_per_sample,
                ..*params
            });
        } else {
            Ok(StreamParams {
                ..*params
            })
        }
    }
}

impl DeviceTrait for Device {
    fn is_default(&self) -> Result<bool> {
        match self {
            #[cfg(windows)]
            Self::Wasapi(device) => device.is_default(),
            #[cfg(unix)]
            Self::Jack(device) => device.is_default(),
            Self::None => Ok(false),
        }
    }

    fn name(&self) -> Result<String> {
        match self {
            #[cfg(windows)]
            Self::Wasapi(device) => device.name(),
            #[cfg(unix)]
            Self::Jack(device) => device.name(),
            Self::None => Ok(String::from("none")),
        }
    }

    fn get_capabilities(&self) -> Result<Capabilities> {
        match self {
            #[cfg(windows)]
            Self::Wasapi(device) => device.get_capabilities(),
            #[cfg(unix)]
            Self::Jack(device) => device.get_capabilities(),
            Self::None => Ok(Capabilities::default()),
        }
    }

    fn start(&mut self, params: &StreamParams) -> Result<Sender<StreamingData>> {
        match self {
            #[cfg(windows)]
            Self::Wasapi(device) => device.start(params),
            #[cfg(unix)]
            Self::Jack(device) => device.start(params),
            Self::None => Err(anyhow!("No host selected")),
        }
    }

    fn pause(&mut self) -> Result<()> {
        match self {
            #[cfg(windows)]
            Self::Wasapi(device) => device.pause(),
            #[cfg(unix)]
            Self::Jack(device) => device.pause(),
            Self::None => Ok(()),
        }
    }

    fn resume(&mut self) -> Result<()> {
        match self {
            #[cfg(windows)]
            Self::Wasapi(device) => device.resume(),
            #[cfg(unix)]
            Self::Jack(device) => device.resume(),
            Self::None => Ok(()),
        }
    }

    fn stop(&mut self) -> Result<()> {
        match self {
            #[cfg(windows)]
            Self::Wasapi(device) => device.stop(),
            #[cfg(unix)]
            Self::Jack(device) => device.stop(),
            Self::None => Ok(()),
        }
    }
}
