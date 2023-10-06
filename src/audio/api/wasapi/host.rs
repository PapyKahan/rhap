use std::sync::Arc;

use wasapi::{DeviceCollection, Direction, get_default_device};
use crate::audio::HostTrait;
use super::{com::com_initialize, device::Device};

#[derive(Clone, Copy)]
pub struct Host {}

impl Host {
    pub(crate) fn new() -> Self {
        Self {}
    }
}

impl HostTrait for Host {
    fn create_device(&self, id: Option<u32>) -> Result<crate::audio::Device, Box<dyn std::error::Error>> {
        com_initialize();
        let devices_collection = DeviceCollection::new(&Direction::Render)?;
        let default_device = get_default_device(&Direction::Render)?;
        let device = match id {
            Some(index) => devices_collection.get_device_at_index(index)?,
            _ => get_default_device(&Direction::Render)?
        };
        let device = Arc::new(device);
        let id = device.get_id()?;
        Ok(crate::audio::Device::Wasapi(Device {
            inner_device: device.clone(),
            is_default: id == default_device.get_id()?,
        }))
    }

    fn get_devices(&self) -> Result<Vec<crate::audio::Device>, Box<dyn std::error::Error>> {
        com_initialize();
        let devices_collection = DeviceCollection::new(&Direction::Render)?;
        let default_device = get_default_device(&Direction::Render)?;
        let mut enumerated_devices : Vec<crate::audio::Device> = vec![];
        for i in 0..devices_collection.get_nbr_devices()? {
            let device = devices_collection.get_device_at_index(i)?;
            let device_id = device.get_id()?;
            enumerated_devices.push(crate::audio::Device::Wasapi(Device {
                inner_device: Arc::new(device),
                is_default: device_id == default_device.get_id()?
            }));
        }
        Ok(enumerated_devices)
    }
}
