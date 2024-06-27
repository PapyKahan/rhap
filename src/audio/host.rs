use super::{Device, api};
use anyhow::Result;


pub trait HostTrait: Send + Sync {
    fn create_device(&self, id: Option<u32>) -> Result<Device>;
    fn get_devices(&self) -> Result<Vec<Device>>;
    fn get_default_device(&self) -> Result<Device>;
}

#[derive(Clone, Copy)]
pub enum Host {
    Wasapi(api::wasapi::host::Host),
}

impl HostTrait for Host {
    fn get_devices(&self) -> Result<Vec<Device>> {
        match self {
            Self::Wasapi(host) => host.get_devices(),
        }
    }

    fn create_device(&self, id: Option<u32>) -> Result<Device> {
        match self {
            Self::Wasapi(host) => host.create_device(id),
        }
    }

    fn get_default_device(&self) -> Result<Device> {
        match self {
            Self::Wasapi(host) => Ok(super::device::Device::Wasapi(host.get_default_device()?)),
        }
    }
}

impl Host {
    pub(crate) fn new(name: &str, high_priority_mode: bool) -> Self {
        match name {
            "wasapi" => Host::Wasapi(api::wasapi::host::Host::new(high_priority_mode)),
            _ => Host::Wasapi(api::wasapi::host::Host::new(high_priority_mode)),
        }
    }
}
