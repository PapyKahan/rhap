use super::Device;
#[cfg(windows)]
use super::api;
#[cfg(unix)]
use super::api;
use anyhow::Result;


pub trait HostTrait: Send + Sync {
    fn create_device(&self, id: Option<u32>) -> Result<Device>;
    fn get_devices(&self) -> Result<Vec<Device>>;
    fn get_default_device(&self) -> Result<Device>;
}

#[derive(Clone, Copy)]
pub enum Host {
    #[cfg(windows)]
    Wasapi(api::wasapi::host::Host),
    #[cfg(unix)]
    Jack(api::jack::host::Host),
}

impl HostTrait for Host {
    fn get_devices(&self) -> Result<Vec<Device>> {
        match self {
            #[cfg(windows)]
            Self::Wasapi(host) => host.get_devices(),
            #[cfg(unix)]
            Self::Jack(host) => host.get_devices(),
        }
    }

    fn create_device(&self, id: Option<u32>) -> Result<Device> {
        match self {
            #[cfg(windows)]
            Self::Wasapi(host) => host.create_device(id),
            #[cfg(unix)]
            Self::Jack(host) => host.create_device(id),
        }
    }

    fn get_default_device(&self) -> Result<Device> {
        match self {
            #[cfg(windows)]
            Self::Wasapi(host) => Ok(super::device::Device::Wasapi(host.get_default_device()?)),
            #[cfg(unix)]
            Self::Jack(host) => host.get_default_device(),
        }
    }
}


impl Host {
    pub(crate) fn new(name: &str, high_priority_mode: bool) -> Self {
        #[cfg(windows)]
        {
            match name {
                "wasapi" => Host::Wasapi(api::wasapi::host::Host::new(high_priority_mode)),
                _ => {
                    // Default to WASAPI on Windows
                    Host::Wasapi(api::wasapi::host::Host::new(high_priority_mode))
                }
            }
        }
        #[cfg(unix)]
        {
            match name {
                "jack" => Host::Jack(api::jack::host::Host::new(high_priority_mode)),
                _ => {
                    // Default to JACK on Unix
                    Host::Jack(api::jack::host::Host::new(high_priority_mode))
                }
            }
        }
    }
}

