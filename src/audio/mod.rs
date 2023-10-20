pub(crate) mod api;

use anyhow::Result;

#[repr(u32)]
#[derive(Debug, Clone, Copy)]
pub enum SampleRate {
    Rate44100Hz = 44100,
    Rate48000Hz = 48000,
    Rate88200Hz = 88200,
    Rate96000Hz = 96000,
    Rate176400Hz = 176400,
    Rate192000Hz = 192000,
}

impl From<u32> for SampleRate {
    fn from(value: u32) -> Self {
        match value {
            44100 => SampleRate::Rate44100Hz,
            48000 => SampleRate::Rate48000Hz,
            88200 => SampleRate::Rate88200Hz,
            96000 => SampleRate::Rate96000Hz,
            176400 => SampleRate::Rate176400Hz,
            192000 => SampleRate::Rate192000Hz,
            _ => panic!("Invalid sample rate"),
        }
    }
}

#[repr(u8)]
#[derive(Debug, Clone, Copy)]
pub enum BitsPerSample {
    Bits8 = 8,
    Bits16 = 16,
    Bits24 = 24,
    Bits32 = 32,
}

impl From<u8> for BitsPerSample {
    fn from(value: u8) -> Self {
        match value {
            8 => BitsPerSample::Bits8,
            16 => BitsPerSample::Bits16,
            24 => BitsPerSample::Bits24,
            32 => BitsPerSample::Bits32,
            _ => panic!("Invalid bits per sample"),
        }
    }
}

#[derive(Debug, Copy, Clone)]
pub struct StreamParams {
    pub channels: u8,
    pub samplerate: SampleRate,
    pub bits_per_sample: BitsPerSample,
    pub buffer_length: i64,
    pub exclusive: bool,
}

pub trait DeviceTrait: Send + Sync {
    fn is_default(&self) -> bool;
    fn is_streaming(&self) -> bool;
    fn set_status(&self, status: StreamingCommand);
    fn get_status(&self) -> StreamingCommand;
    fn name(&self) -> String;
    fn start(&mut self, context: StreamContext) -> Result<(), Box<dyn std::error::Error>>;
    fn stop(&self);
    fn send(&self, i: u8) -> Result<(), std::sync::mpsc::SendError<u8>>;
}

#[derive(Clone)]
pub enum Device {
    None,
    Wasapi(api::wasapi::device::Device),
}

impl Device {
}

#[derive(Copy, Clone, PartialEq, Eq)]
pub enum StreamingCommand {
    Start,
    Stop,
    Pause,
}

#[derive(Clone)]
pub struct StreamContext {
    parameters: StreamParams,
}

impl StreamContext {
    pub fn new(parameters: StreamParams) -> Self {
        Self {
            parameters,
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

    fn start(&mut self, context: StreamContext) -> Result<(), Box<dyn std::error::Error>> {
        let device = match self {
            Self::Wasapi(device) => device,
            Self::None => return Ok(()),
        };
        device.start(context)
    }

    fn is_streaming(&self) -> bool {
        let device = match self {
            Self::Wasapi(device) => device,
            Self::None => return false,
        };
        device.is_streaming()
    }

    fn set_status(&self, status: StreamingCommand) {
        let device = match self {
            Self::Wasapi(device) => device,
            Self::None => return,
        };
        device.set_status(status)
    }

    fn get_status(&self) -> StreamingCommand {
        let device = match self {
            Self::Wasapi(device) => device,
            Self::None => return StreamingCommand::Stop,
        };
        device.get_status()
    }

    fn stop(&self) {
        let device = match self {
            Self::Wasapi(device) => device,
            Self::None => return,
        };
        device.stop()
    }

    fn send(&self, i: u8) -> Result<(), std::sync::mpsc::SendError<u8>> {
        let device = match self {
            Self::Wasapi(device) => device,
            Self::None => return Ok(()),
        };
        device.send(i)
    }
}

pub trait HostTrait: Send + Sync {
    fn create_device(&self, id: Option<u32>) -> Result<Device, Box<dyn std::error::Error>>;
    fn get_devices(&self) -> Result<Vec<Device>, Box<dyn std::error::Error>>;
    fn get_default_device(&self) -> Result<Device, Box<dyn std::error::Error>>;
}

#[derive(Clone, Copy)]
pub enum Host {
    Wasapi(api::wasapi::host::Host),
}

impl HostTrait for Host {
    fn get_devices(&self) -> Result<Vec<Device>, Box<dyn std::error::Error>> {
        match self {
            Self::Wasapi(host) => host.get_devices(),
        }
    }

    fn create_device(&self, id: Option<u32>) -> Result<Device, Box<dyn std::error::Error>> {
        match self {
            Self::Wasapi(host) => host.create_device(id),
        }
    }

    fn get_default_device(&self) -> Result<Device, Box<dyn std::error::Error>> {
        match self {
            Self::Wasapi(host) => host.get_default_device(),
        }
    }
}

impl Host {
    pub(crate) fn new(name: &str) -> Self {
        match name {
            "wasapi" => Host::Wasapi(api::wasapi::host::Host::new()),
            _ => Host::Wasapi(api::wasapi::host::Host::new()),
        }
    }
}
