use wasapi::{DeviceCollection, Direction, get_default_device};

use super::{com::com_initialize, device::Device};

pub struct Host {
}

impl Host {
    pub(crate) fn get_device(id: Option<u32>) -> Result<Device, Box<dyn std::error::Error>> {
        com_initialize();
        let devices_collection = DeviceCollection::new(&Direction::Render)?;
        let default_device = get_default_device(&Direction::Render)?;
        let device = match id {
            Some(index) => devices_collection.get_device_at_index(index)?,
            _ => get_default_device(&Direction::Render)?
        };
        let id = device.get_id()?;
        Ok(Device {
            inner_device: device,
            is_default: id == default_device.get_id()?,
        })
    }

    pub(crate) fn get_devices() -> Result<Vec<Device>, Box<dyn std::error::Error>> {
        com_initialize();
        let devices_collection = DeviceCollection::new(&Direction::Render)?;
        let default_device = get_default_device(&Direction::Render)?;
        let mut enumerated_devices = vec![];
        for i in 0..devices_collection.get_nbr_devices()? {
            let device = devices_collection.get_device_at_index(i)?;
            let device_id = device.get_id()?;
            enumerated_devices.push(Device {
                inner_device: device,
                is_default: device_id == default_device.get_id()?
            });
        }
        Ok(enumerated_devices)
    }
}
