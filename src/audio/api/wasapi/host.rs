use super::{api::com_initialize, device::Device};
use crate::audio::HostTrait;
use anyhow::Result;
use windows::Win32::{
    Media::Audio::{
        eMultimedia, eRender, IMMDeviceEnumerator, MMDeviceEnumerator, DEVICE_STATE_ACTIVE,
    },
    System::Com::{CoCreateInstance, CLSCTX_ALL},
};

#[derive(Clone, Copy)]
pub struct Host {
    high_priority_mode: bool,
}

impl Host {
    pub(crate) fn new(high_priority_mode: bool) -> Self {
        Self { high_priority_mode }
    }

    pub fn get_default_device(&self) -> Result<Device> {
        com_initialize();
        let enumerator: IMMDeviceEnumerator =
            unsafe { CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)? };
        let device = unsafe { enumerator.GetDefaultAudioEndpoint(eRender, eMultimedia)? };
        let default_device_id = unsafe { device.GetId()?.to_string()? };
        Ok(Device::new(device, default_device_id, self.high_priority_mode)?)
    }
}

impl HostTrait for Host {
    fn create_device(&self, id: Option<u32>) -> Result<crate::audio::Device> {
        com_initialize();
        let enumerator: IMMDeviceEnumerator =
            unsafe { CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)? };

        let devices_collection =
            unsafe { enumerator.EnumAudioEndpoints(eRender, DEVICE_STATE_ACTIVE)? };

        let default_device = self.get_default_device()?;
        let default_device_id = default_device.get_id()?;
        let device = match id {
            Some(index) => Device::new(
                unsafe { devices_collection.Item(index)? },
                default_device_id,
                self.high_priority_mode
            )?,
            _ => default_device,
        };
        Ok(crate::audio::Device::Wasapi(device))
    }

    fn get_devices(&self) -> Result<Vec<crate::audio::Device>> {
        com_initialize();
        let enumerator: IMMDeviceEnumerator =
            unsafe { CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)? };
        let devices_collection =
            unsafe { enumerator.EnumAudioEndpoints(eRender, DEVICE_STATE_ACTIVE)? };
        let default_device = self.get_default_device()?;
        let default_device_id = default_device.get_id()?;

        let mut enumerated_devices: Vec<crate::audio::Device> = vec![];

        for i in 0..unsafe { devices_collection.GetCount()? } {
            let inner_device = unsafe { devices_collection.Item(i)? };
            let device = Device::new(inner_device, default_device_id.clone(), self.high_priority_mode)?;
            enumerated_devices.push(crate::audio::Device::Wasapi(device));
        }
        Ok(enumerated_devices)
    }

    fn get_default_device(&self) -> Result<crate::audio::Device> {
        Ok(crate::audio::Device::Wasapi(self.get_default_device()?))
    }
}
