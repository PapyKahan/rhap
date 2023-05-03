use windows::{
    core::PCWSTR,
    Win32::{
        Media::Audio::{
            eRender, IMMDevice, IMMDeviceCollection, IMMDeviceEnumerator,
            MMDeviceEnumerator, DEVICE_STATE_ACTIVE, eMultimedia,
        },
        System::Com::{
            CoCreateInstance, CoInitializeEx,
            CLSCTX_ALL, COINIT_MULTITHREADED,
        },
    },
};

use super::device::Device;

pub struct Host;
impl Host {
    pub fn enumerate_devices() -> Result<Vec<Device>, String> {
        let mut enumerated_devices = vec![];
        unsafe {
            // Initialise les sous-systèmes COM
            match CoInitializeEx(None, COINIT_MULTITHREADED) {
                Ok(_) => (),
                Err(err) => {
                    return Err(format!("Error initialising COM: {}", err));
                }
            };

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
