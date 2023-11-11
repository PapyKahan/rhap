use super::{com::com_initialize, device::Device};
use crate::audio::HostTrait;
use anyhow::{anyhow, Result};
use wasapi::{get_default_device, DeviceCollection, Direction};

#[derive(Clone, Copy)]
pub struct Host {}

impl Host {
    pub(crate) fn new() -> Self {
        Self {}
    }
}

impl HostTrait for Host {
    fn create_device(&self, id: Option<u32>) -> Result<crate::audio::Device> {
        com_initialize();
        let devices_collection = DeviceCollection::new(&Direction::Render)
            .map_err(|e| anyhow!("DeviceCollection::new failed: {}", e))?;
        let default_device = get_default_device(&Direction::Render)
            .map_err(|e| anyhow!("get_default_device failed: {}", e))?;
        let device = match id {
            Some(index) => devices_collection
                .get_device_at_index(index)
                .map_err(|e| anyhow!("DeviceCollection::get_device_at_index failed: {}", e))?,
            _ => get_default_device(&Direction::Render)
                .map_err(|e| anyhow!("get_default_device failed: {}", e))?,
        };
        let device_id = device
            .get_id()
            .map_err(|e| anyhow!("Device::get_id failed: {}", e))?;
        Ok(crate::audio::Device::Wasapi(Device::new(
            device,
            device_id
                == default_device
                    .get_id()
                    .map_err(|e| anyhow!("Device::get_id failed: {}", e))?,
        )?))
    }

    fn get_devices(&self) -> Result<Vec<crate::audio::Device>> {
        com_initialize();
        let devices_collection = DeviceCollection::new(&Direction::Render)
            .map_err(|e| anyhow!("DeviceCollection::new failed: {}", e))?;
        let default_device = get_default_device(&Direction::Render)
            .map_err(|e| anyhow!("get_default_device failed: {}", e))?;
        let mut enumerated_devices: Vec<crate::audio::Device> = vec![];
        for i in 0..devices_collection
            .get_nbr_devices()
            .map_err(|e| anyhow!("DeviceCollection::get_nbr_devices failed: {}", e))?
        {
            let device = devices_collection
                .get_device_at_index(i)
                .map_err(|e| anyhow!("DeviceCollection::get_device_at_index failed: {}", e))?;
            let device_id = device
                .get_id()
                .map_err(|e| anyhow!("Device::get_id failed: {}", e))?;
            enumerated_devices.push(crate::audio::Device::Wasapi(Device::new(
                device,
                device_id
                    == default_device
                        .get_id()
                        .map_err(|e| anyhow!("Device::get_id failed: {}", e))?,
            )?));
        }
        Ok(enumerated_devices)
    }

    fn get_default_device(&self) -> Result<crate::audio::Device> {
        com_initialize();
        let device = get_default_device(&Direction::Render)
            .map_err(|e| anyhow!("get_default_device failed: {}", e))?;
        Ok(crate::audio::Device::Wasapi(Device::new(device, true)?))
    }
}
