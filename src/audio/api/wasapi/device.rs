use std::{ffi::OsString, os::windows::prelude::OsStringExt, slice};

use crate::audio::{DeviceTrait, StreamParams};

use super::{host::Host, stream::Stream};
use windows::{
    core::PCWSTR,
    Win32::{
        Devices::FunctionDiscovery::PKEY_Device_FriendlyName,
        Media::Audio::{IMMDevice, IMMDeviceEnumerator, MMDeviceEnumerator},
        System::Com::{
            CoCreateInstance, CoInitializeEx, CoUninitialize, StructuredStorage::PropVariantClear,
            CLSCTX_ALL, COINIT_MULTITHREADED, STGM_READ, VT_LPWSTR,
        },
        UI::Shell::PropertiesSystem::IPropertyStore,
    },
};

pub struct Device {
    pub index: u32,
    pub is_default: bool,
    pub(super) device: IMMDevice,
}

impl DeviceTrait for Device {
    fn get_name(&self) -> String {
        match Self::get_device_name(&self.device) {
            Ok(name) => name,
            Err(_) => String::from(""),
        }
    }

    fn new(id: u32) -> Result<Self, String> {
        let selected_device = match Self::get_device(id) {
            Ok(device) => device,
            Err(err) => {
                return Err(format!("Error getting device: {}", err));
            }
        };
        unsafe {
            match CoInitializeEx(None, COINIT_MULTITHREADED) {
                Ok(_) => (),
                Err(err) => {
                    return Err(format!("Error initializing COM: {} - {}", err.code(), err))
                }
            };

            let enumerator: IMMDeviceEnumerator =
                match CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL) {
                    Ok(device_enumerator) => device_enumerator,
                    Err(err) => {
                        return Err(format!(
                            "Error getting device enumerator: {} - {}",
                            err.code(),
                            err
                        ));
                    }
                };

            let device = match enumerator.GetDevice(selected_device) {
                Ok(device) => device,
                Err(err) => {
                    return Err(format!("Error getting device: {} - {}", err.code(), err));
                }
            };

            Ok(Self {
                device,
                index: id,
                is_default: false,
            })
        }
    }

    fn build_stream<T>(
        &self,
        params: StreamParams,
        callback: T,
    ) -> Result<Box<dyn crate::audio::StreamTrait>, String>
    where
        T: FnMut(&mut [u8], usize) -> Result<crate::audio::StreamFlow, String> + Send + 'static,
    {
        let stream = match Stream::build_from_device(&self.device, params, callback) {
            Ok(stream) => stream,
            Err(err) => {
                return Err(err);
            }
        };
        Ok(Box::new(stream))
    }
}

impl Device {
    fn get_device_name(device: &IMMDevice) -> Result<String, String> {
        unsafe {
            let property_store: IPropertyStore = (*device).OpenPropertyStore(STGM_READ).unwrap();
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
            Ok(name)
        }
    }

    pub fn get_device(id: u32) -> Result<PCWSTR, String> {
        let devices = match Host::enumerate_devices() {
            Ok(devices) => devices,
            Err(err) => {
                println!("Error enumerating devices: {}", err);
                return Err(err);
            }
        };

        for dev in devices {
            if dev.index == id {
                return Ok(dev.get_id());
            }
        }
        Err("Device not found".to_string())
    }

    fn get_id(&self) -> PCWSTR {
        unsafe { PCWSTR::from_raw(self.device.GetId().unwrap().as_ptr()) }
    }

    pub fn get_capabilities(&self) -> Result<(), String> {
        unsafe {
            // Initialise les sous-systèmes COM
            match CoInitializeEx(None, COINIT_MULTITHREADED) {
                Ok(_) => (),
                Err(_) => {
                    return Err("Error initialising COM".to_string());
                }
            };

            //let device = Self::get_device(self.index).unwrap();

            //// Get Device capabilities
            //let audio_client: IAudioClient = match device.Activate(CLSCTX_ALL, None) {
            //    Ok(audio_client) => audio_client,
            //    Err(err) => {
            //        return Err("Error getting audio client".to_string());
            //    }
            //};

            //let wave_format = Stream::create_waveformat_from(StreamParams {
            //    device: crate::audio::Device {
            //        id: 0,
            //        name: String::from("")
            //    },
            //    channels: 2,
            //    samplerate: crate::audio::SampleRate::Rate44100Hz,
            //    bits_per_sample: crate::audio::BitsPerSample::Bits16,
            //    buffer_length: 0,
            //    exclusive: false,
            //});

            //let sharemode = match true {
            //    true => AUDCLNT_SHAREMODE_EXCLUSIVE,
            //    false => AUDCLNT_SHAREMODE_SHARED,
            //};
            //match audio_client.IsFormatSupported(
            //    sharemode,
            //    &wave_format.Format as *const WAVEFORMATEX,
            //    None,
            //) {
            //    S_OK => true,
            //    result => {
            //        return Err(format!(
            //            "Error checking format support: {} - {}",
            //            host_error(result),
            //            "Unsuported format"
            //        ));
            //    }
            //};
            CoUninitialize();
        }
        Ok(())
    }
}
