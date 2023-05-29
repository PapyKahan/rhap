use windows::{
    core::PCWSTR,
    Win32::{
        Media::Audio::{
            eMultimedia, eRender, IMMDevice, IMMDeviceCollection, IMMDeviceEnumerator,
            MMDeviceEnumerator, DEVICE_STATE_ACTIVE,
        },
        System::Com::{CoCreateInstance, CLSCTX_ALL},
    },
};

use super::{device::Device, com::com_initialize};

#[derive(Debug, Copy, Clone)]
pub struct Host {
}

impl Host {
    pub(crate) fn new() -> Result<Self, String> {
        com_initialize();
        Ok(Self {})
    }

    pub(crate) fn create_device(&self, id: Option<u32>) -> Result<Device, String> {
        match Device::new(id) {
            Ok(device) => Ok(device),
            Err(e) => Err(format!("Failed to open device: {}", e)),
        }
    }

    pub(crate) fn get_devices(&self) -> Result<Vec<Device>, String> {
        match Self::enumerate_devices() {
            Ok(devices) => Ok(devices),
            Err(e) => Err(format!("Failed to enumerate devices: {}", e)),
        }
    }

    pub(super) fn enumerate_devices() -> Result<Vec<Device>, String> {
        let mut enumerated_devices = vec![];
        unsafe {
            let enumerator: IMMDeviceEnumerator =
                match CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL) {
                    Ok(device_enumerator) => device_enumerator,
                    Err(err) => {
                        return Err(format!("Error getting device enumerator: {}", err));
                    }
                };

            let devices: IMMDeviceCollection =
                match enumerator.EnumAudioEndpoints(eRender, DEVICE_STATE_ACTIVE) {
                    Ok(devices) => devices,
                    Err(err) => {
                        return Err(format!("Error getting device list: {}", err));
                    }
                };

            let default_device = match enumerator.GetDefaultAudioEndpoint(eRender, eMultimedia) {
                Ok(device) => device,
                Err(err) => {
                    return Err(format!("Error getting default device: {}", err));
                }
            };

            let default_device_id = match default_device.GetId() {
                Ok(id) => PCWSTR::from_raw(id.as_ptr()),
                Err(err) => {
                    return Err(format!("Error getting default device id: {}", err));
                }
            };

            let default_device_id_string = match default_device_id.to_string() {
                Ok(id) => id,
                Err(err) => {
                    return Err(format!("Error converting default device id: {}", err));
                }
            };

            for index in 0..devices.GetCount().unwrap() {
                let device: IMMDevice = match devices.Item(index) {
                    Ok(device) => device,
                    Err(err) => {
                        return Err(format!("Error getting device: {}", err));
                    }
                };

                let id = match device.GetId() {
                    Ok(id) => PCWSTR::from_raw(id.as_ptr()),
                    Err(err) => {
                        return Err(format!("Error getting device id: {}", err));
                    }
                };

                let device_id_string = match id.to_string() {
                    Ok(id) => id,
                    Err(err) => {
                        return Err(format!("Error converting device id: {}", err));
                    }
                };

                let is_default = device_id_string == default_device_id_string;

                enumerated_devices.push(Device {
                    index,
                    device,
                    is_default,
                });
            }

            Ok(enumerated_devices)
        }
    }
}
