use super::{api, Capabilities, StreamParams, StreamingData};
use anyhow::{anyhow, Result};
use tokio::sync::mpsc::Sender;

pub trait DeviceTrait: Send + Sync {
    fn is_default(&self) -> bool;
    fn name(&self) -> String;
    fn get_capabilities(&self) -> Result<Capabilities>;
    fn start(&mut self, params: StreamParams) -> Result<Sender<StreamingData>>;
    fn pause(&mut self) -> Result<()>;
    fn resume(&mut self) -> Result<()>;
    fn stop(&mut self) -> Result<()>;
}

pub enum Device {
    None,
    Wasapi(api::wasapi::device::Device),
}

impl Device {
    pub fn adjust_stream_params(&self, params: StreamParams) -> Result<StreamParams> {
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
                ..params
            });
        } else {
            Ok(params)
        }
    }
}

impl DeviceTrait for Device {
    fn is_default(&self) -> bool {
        let device = match self {
            Self::Wasapi(device) => device,
            Self::None => return false,
        };
        device.is_default()
    }

    fn name(&self) -> String {
        let device = match self {
            Self::Wasapi(device) => device,
            Self::None => return String::from("none"),
        };
        device.name()
    }

    fn get_capabilities(&self) -> Result<Capabilities> {
        let device = match self {
            Self::Wasapi(device) => device,
            Self::None => return Ok(Capabilities::default()),
        };
        device.get_capabilities()
    }

    fn start(&mut self, params: StreamParams) -> Result<Sender<StreamingData>> {
        let device = match self {
            Self::Wasapi(device) => device,
            Self::None => return Err(anyhow!("No host selected")),
        };
        device.start(params)
    }

    fn pause(&mut self) -> Result<()> {
        let device = match self {
            Self::Wasapi(device) => device,
            Self::None => return Ok(()),
        };
        device.pause()
    }

    fn resume(&mut self) -> Result<()> {
        let device = match self {
            Self::Wasapi(device) => device,
            Self::None => return Ok(()),
        };
        device.resume()
    }

    fn stop(&mut self) -> Result<()> {
        let device = match self {
            Self::Wasapi(device) => device,
            Self::None => return Ok(()),
        };
        device.stop()
    }
}
