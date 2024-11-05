use super::{api, Capabilities, StreamParams};
use anyhow::Result;

pub trait DeviceTrait: Send + Sync {
    fn is_default(&self) -> Result<bool>;
    fn name(&self) -> Result<String>;
    fn get_capabilities(&self) -> Result<Capabilities>;
    fn start(&mut self, params: &StreamParams) -> Result<()>;
    fn write(&mut self, data: &[u8]) -> Result<()>;
    fn stop(&mut self) -> Result<()>;
}

pub enum Device {
    None,
    Wasapi(api::wasapi::device::Device),
}

impl Device {
    pub fn adjust_stream_params(&self, params: &StreamParams) -> Result<StreamParams> {
        let capabilities = self.get_capabilities()?;
        let contains_sample_rates = capabilities.sample_rates.contains(&params.samplerate);
        let contains_bits_per_samples = capabilities
            .bits_per_samples
            .contains(&params.bits_per_sample);
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
            Ok(StreamParams { ..*params })
        }
    }
}

impl DeviceTrait for Device {
    fn is_default(&self) -> Result<bool> {
        let device = match self {
            Self::Wasapi(device) => device,
            Self::None => return Ok(false),
        };
        device.is_default()
    }

    fn name(&self) -> Result<String> {
        let device = match self {
            Self::Wasapi(device) => device,
            Self::None => return Ok(String::from("none")),
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

    fn start(&mut self, params: &StreamParams) -> Result<()> {
        let device = match self {
            Self::Wasapi(device) => device,
            Self::None => return Ok(()),
        };
        device.start(params)
    }

    fn write(&mut self, data: &[u8]) -> Result<()> {
        let device = match self {
            Self::Wasapi(device) => device,
            Self::None => return Ok(()),
        };
        device.write(data)
    }

    fn stop(&mut self) -> Result<()> {
        let device = match self {
            Self::Wasapi(device) => device,
            Self::None => return Ok(()),
        };
        device.stop()
    }
}
