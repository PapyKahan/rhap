mod utils;
mod device;
pub mod stream;

use std::{ffi::OsString, os::windows::prelude::OsStringExt, slice};
use windows::{Win32::{System::Com::{CoInitializeEx, COINIT_MULTITHREADED, CLSCTX_ALL, STGM_READ, VT_LPWSTR, CoCreateInstance, StructuredStorage::PropVariantClear}, Media::Audio::{IMMDeviceEnumerator, MMDeviceEnumerator, IMMDeviceCollection, DEVICE_STATE_ACTIVE, eRender, IMMDevice}, UI::Shell::PropertiesSystem::IPropertyStore, Devices::FunctionDiscovery::PKEY_Device_FriendlyName}, core::PCWSTR};

use self::device::Device;

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

        for index in 0..devices.GetCount().unwrap() {
            let device: IMMDevice = match devices.Item(index) {
                Ok(device) => device,
                Err(err) => {
                    println!("Error getting device: {}", err);
                    return Err("Error getting device".to_string());
                }
            };

            let property_store: IPropertyStore = device.OpenPropertyStore(STGM_READ).unwrap();
            let mut name_property_value = match property_store.GetValue(&PKEY_Device_FriendlyName) {
                Ok(name_property_value) => name_property_value,
                Err(err) => {
                    println!("Error getting device name: {}", err);
                    return Err("Error getting device name".to_string());
                }
            };

            let prop_variant = &name_property_value.Anonymous.Anonymous;

            // Read the friendly-name from the union data field, expecting a *const u16.
            if prop_variant.vt != VT_LPWSTR {
                let description = format!(
                    "property store produced invalid data: {:?}",
                    prop_variant.vt
                );
                return Err(description);
            }
            let ptr_utf16 = *(&prop_variant.Anonymous as *const _ as *const *const u16);

            // Find the length of the friendly name.
            let mut len = 0;
            while *ptr_utf16.offset(len) != 0 {
                len += 1;
            }

            // Create the utf16 slice and convert it into a string.
            let name_slice = slice::from_raw_parts(ptr_utf16, len as usize);
            let name_os_string: OsString = OsStringExt::from_wide(name_slice);
            let name = match name_os_string.into_string() {
                Ok(string) => string,
                Err(os_string) => os_string.to_string_lossy().into(),
            };

            // Clean up the property.
            match PropVariantClear(&mut name_property_value) {
                Ok(_) => (),
                Err(err) => {
                    println!("Error clearing property: {}", err);
                    return Err("Error clearing property".to_string());
                }
            };

            let id = match device.GetId() {
                Ok(id) => PCWSTR::from_raw(id.as_ptr()),
                Err(err) => {
                    println!("Error getting device id: {}", err);
                    return Err("Error getting device id".to_string());
                }
            };

            enumerated_devices.push(Device::new(id, index as u16, name));
        }

        Ok(enumerated_devices)
    }
}
