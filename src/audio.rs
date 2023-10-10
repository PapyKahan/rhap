use std::{sync::{Arc, Mutex}, collections::VecDeque};

use anyhow::Result;
pub mod api;

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
    fn is_playing(&self) -> bool;
    fn set_status(&self, status: PlaybackCommand);
    fn get_status(&self) -> PlaybackCommand;
    fn name(&self) -> String;
    fn stream(&mut self, context: StreamContext) -> Result<(), Box<dyn std::error::Error>>;
    fn stop(&self);
}

#[derive(Clone)]
pub enum Device {
    Wasapi(api::wasapi::device::Device),
}

#[derive(Copy, Clone)]
pub enum PlaybackCommand {
    Play,
    Stop,
    Pause,
}

#[derive(Clone)]
pub struct StreamContext {
    source: Arc<Mutex<VecDeque<u8>>>,
    parameters: StreamParams,
}

impl StreamContext {
    pub fn new(source: Arc<Mutex<VecDeque<u8>>>, parameters: StreamParams) -> Self {
        Self {
            source,
            parameters,
        }
    }
}

impl DeviceTrait for Device {
    fn is_default(&self) -> bool {
        let device = match self {
            Self::Wasapi(device) => device,
        };
        device.is_default()
    }

    fn name(&self) -> String {
        let device = match self {
            Self::Wasapi(device) => device,
        };
        device.name()
    }

    fn stream(&mut self, context: StreamContext) -> Result<(), Box<dyn std::error::Error>> {
        let device = match self {
            Self::Wasapi(device) => device,
        };
        device.stream(context)
    }

    fn is_playing(&self) -> bool {
        let device = match self {
            Self::Wasapi(device) => device,
        };
        device.is_playing()
    }

    fn set_status(&self, status: PlaybackCommand) {
        let device = match self {
            Self::Wasapi(device) => device,
        };
        device.set_status(status)
    }

    fn get_status(&self) -> PlaybackCommand {
        let device = match self {
            Self::Wasapi(device) => device,
        };
        device.get_status()
    }

    fn stop(&self) {
        let device = match self {
            Self::Wasapi(device) => device,
        };
        device.stop()
    }
}

pub trait HostTrait: Send + Sync {
    fn create_device(&self, id: Option<u32>) -> Result<Device, Box<dyn std::error::Error>>;
    fn get_devices(&self) -> Result<Vec<Device>, Box<dyn std::error::Error>>;
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
}

pub(crate) fn create_host(host_name: &str) -> Host {
    match host_name {
        "wasapi" => Host::Wasapi(api::wasapi::host::Host::new()),
        _ => Host::Wasapi(api::wasapi::host::Host::new()),
    }
}
