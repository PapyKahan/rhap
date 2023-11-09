use std::sync::mpsc::SyncSender;
use anyhow::{anyhow, Result};

use super::{StreamParams, StreamingCommand, api};

pub trait DeviceTrait: Send + Sync {
    fn is_default(&self) -> bool;
    //fn set_status(&self, status: StreamingCommand);
    //fn get_status(&self) -> StreamingCommand;
    fn name(&self) -> String;
    fn start(&mut self, params: StreamParams) -> Result<SyncSender<StreamingCommand>>;
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

    fn start(&mut self, params: StreamParams) -> Result<SyncSender<StreamingCommand>> {
        let device = match self {
            Self::Wasapi(device) => device,
            Self::None => return Err(anyhow!("No host selected")),
        };
        device.start(params)
    }

    fn stop(&mut self) -> Result<()> {
        let device = match self {
            Self::Wasapi(device) => device,
            Self::None => return Ok(()),
        };
        device.stop()
    }
}

