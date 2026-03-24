use super::{Device, api};
use anyhow::Result;


pub trait HostTrait: Send + Sync {
    fn create_device(&self, id: Option<u32>) -> Result<Device>;
    fn get_devices(&self) -> Result<Vec<Device>>;
    fn get_default_device(&self) -> Result<Device>;
}

#[derive(Clone, Copy)]
pub enum Host {
    #[cfg(target_os = "windows")]
    Wasapi(api::wasapi::host::Host),
    #[cfg(target_os = "linux")]
    Alsa(api::alsa::host::Host),
    #[cfg(target_os = "linux")]
    PipeWire(api::pipewire::host::Host),
}

impl HostTrait for Host {
    fn get_devices(&self) -> Result<Vec<Device>> {
        match self {
            #[cfg(target_os = "windows")]
            Self::Wasapi(host) => host.get_devices(),
            #[cfg(target_os = "linux")]
            Self::Alsa(host) => host.get_devices(),
            #[cfg(target_os = "linux")]
            Self::PipeWire(host) => host.get_devices(),
        }
    }

    fn create_device(&self, id: Option<u32>) -> Result<Device> {
        match self {
            #[cfg(target_os = "windows")]
            Self::Wasapi(host) => host.create_device(id),
            #[cfg(target_os = "linux")]
            Self::Alsa(host) => host.create_device(id),
            #[cfg(target_os = "linux")]
            Self::PipeWire(host) => host.create_device(id),
        }
    }

    fn get_default_device(&self) -> Result<Device> {
        match self {
            #[cfg(target_os = "windows")]
            Self::Wasapi(host) => Ok(super::device::Device::Wasapi(host.get_default_device()?)),
            #[cfg(target_os = "linux")]
            Self::Alsa(host) => host.get_default_device(),
            #[cfg(target_os = "linux")]
            Self::PipeWire(host) => host.get_default_device(),
        }
    }
}

impl Host {
    pub(crate) fn new(name: &str, high_priority_mode: bool) -> Self {
        match name {
            #[cfg(target_os = "windows")]
            "wasapi" => Host::Wasapi(api::wasapi::host::Host::new(high_priority_mode)),
            #[cfg(target_os = "linux")]
            "alsa" => Host::Alsa(api::alsa::host::Host::new(high_priority_mode)),
            #[cfg(target_os = "linux")]
            "pipewire" => Host::PipeWire(api::pipewire::host::Host),
            _ => Self::default_host(high_priority_mode),
        }
    }

    fn default_host(high_priority_mode: bool) -> Self {
        #[cfg(target_os = "windows")]
        { Host::Wasapi(api::wasapi::host::Host::new(high_priority_mode)) }
        #[cfg(target_os = "linux")]
        {
            // Auto-detect: prefer PipeWire if running, fall back to ALSA
            let xdg = std::env::var("XDG_RUNTIME_DIR").unwrap_or_default();
            if std::path::Path::new(&format!("{}/pipewire-0", xdg)).exists() {
                Host::PipeWire(api::pipewire::host::Host)
            } else {
                Host::Alsa(api::alsa::host::Host::new(high_priority_mode))
            }
        }
    }
}
