use std::sync::mpsc::SyncSender;

use super::{api, Capabilities, StreamParams};
use anyhow::{anyhow, Result};

pub trait DeviceTrait: Send + Sync {
    fn is_default(&self) -> bool;
    fn name(&self) -> String;
    fn get_capabilities(&self) -> Result<Capabilities>;
    fn start(&mut self, params: StreamParams) -> Result<SyncSender<u8>>;
    fn pause(&mut self) -> Result<()>;
    fn resume(&mut self) -> Result<()>;
    fn stop(&mut self) -> Result<()>;
}

pub enum Device {
    None,
    Wasapi(api::wasapi::device::Device),
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

    fn start(&mut self, params: StreamParams) -> Result<SyncSender<u8>> {
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
