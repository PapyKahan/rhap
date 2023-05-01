use windows::{
    core::PCWSTR,
    Win32::{
        Media::Audio::{
            eRender, IMMDevice, IMMDeviceCollection, IMMDeviceEnumerator,
            MMDeviceEnumerator, DEVICE_STATE_ACTIVE, eMultimedia,
        },
        System::Com::{
            CoCreateInstance, CoInitializeEx, CoUninitialize,
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
                    println!("Error initialising COM: {}", err);
                    return Err("Error initialising COM".to_string());
                }
            };

            let enumerator: IMMDeviceEnumerator =
                match CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL) {
                    Ok(device_enumerator) => device_enumerator,
                    Err(err) => {
                        println!("Error getting device enumerator: {}", err);
                        return Err("Error getting device enumerator".to_string());
                    }
                };

            let devices: IMMDeviceCollection =
                match enumerator.EnumAudioEndpoints(eRender, DEVICE_STATE_ACTIVE) {
                    Ok(devices) => devices,
                    Err(err) => {
                        println!("Error getting device list: {}", err);
                        return Err("Error getting device list".to_string());
                    }
                };

            let default_device = match enumerator.GetDefaultAudioEndpoint(eRender, eMultimedia) {
                Ok(device) => device,
                Err(err) => {
                    println!("Error getting default device: {}", err);
                    return Err("Error getting default device".to_string());
                }
            };

            for index in 0..devices.GetCount().unwrap() {
                let device: IMMDevice = match devices.Item(index) {
                    Ok(device) => device,
                    Err(err) => {
                        println!("Error getting device: {}", err);
                        return Err("Error getting device".to_string());
                    }
                };

                let id = match device.GetId() {
                    Ok(id) => PCWSTR::from_raw(id.as_ptr()),
                    Err(err) => {
                        println!("Error getting device id: {}", err);
                        return Err("Error getting device id".to_string());
                    }
                };

                let is_default = id == PCWSTR::from_raw(default_device.GetId().unwrap().as_ptr());

                enumerated_devices.push(Device {
                    index,
                    device,
                    is_default,
                });
            }

            CoUninitialize();

            Ok(enumerated_devices)
        }
    }
}
