use crate::audio::{HostTrait, Device};
use anyhow::Result;

#[derive(Clone, Copy)]
pub struct Host {
    high_priority_mode: bool,
}

impl Host {
    pub(crate) fn new(high_priority_mode: bool) -> Self {
        Self { high_priority_mode }
    }
}

impl HostTrait for Host {
    fn get_devices(&self) -> Result<Vec<Device>> {
        #[cfg(unix)]
        {
            // JACK typically has one default output device
            // We'll create a single device representing the JACK output
            let device = super::device::Device::new("default", self.high_priority_mode)?;
            Ok(vec![Device::Jack(device)])
        }
        
        #[cfg(not(unix))]
        {
            Ok(vec![])
        }
    }

    fn create_device(&self, id: Option<u32>) -> Result<Device> {
        #[cfg(unix)]
        {
            match id {
                Some(0) | None => {
                    let device = super::device::Device::new("default", self.high_priority_mode)?;
                    Ok(Device::Jack(device))
                }
                Some(id) => Err(anyhow::anyhow!("JACK device ID {} not found. Only device 0 (default) is available.", id)),
            }
        }
        
        #[cfg(not(unix))]
        {
            Err(anyhow::anyhow!("JACK is only supported on Unix systems"))
        }
    }

    fn get_default_device(&self) -> Result<Device> {
        self.create_device(None)
    }
}