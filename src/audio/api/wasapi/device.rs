use std::{ffi::OsString, os::windows::prelude::OsStringExt, slice};
use windows::Win32::{
    Devices::FunctionDiscovery::PKEY_Device_FriendlyName,
    Media::Audio::{eMultimedia, eRender, IMMDevice, IMMDeviceEnumerator, MMDeviceEnumerator, IAudioClient, AUDCLNT_SHAREMODE_EXCLUSIVE, AUDCLNT_SHAREMODE_SHARED, WAVEFORMATEX},
    System::Com::{
        CoCreateInstance, StructuredStorage::PropVariantClear, CLSCTX_ALL,
        STGM_READ, VT_LPWSTR,
    },
    UI::Shell::PropertiesSystem::IPropertyStore, Foundation::S_OK,
};

use super::{host::Host, stream::Stream, utils::host_error};
use crate::audio::{DeviceTrait, StreamParams};

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
    pub(super) fn new(id: Option<u32>) -> Result<Self, String> {
        let device = match id {
            Some(id) => match Self::get_device(id) {
                Ok(device) => device,
                Err(err) => {
                    return Err(format!("Error getting device: {}", err));
                }
            },
            _ => match Self::get_default_device() {
                Ok(device) => device,
                Err(err) => {
                    return Err(format!("Error getting default device: {}", err));
                }
            },
        };

        Ok(Self {
            device,
            index: Option::unwrap_or(id, 0),
            is_default: false,
        })
    }

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

    fn get_device(id: u32) -> Result<IMMDevice, String> {
        let devices = match Host::enumerate_devices() {
            Ok(devices) => devices,
            Err(err) => {
                println!("Error enumerating devices: {}", err);
                return Err(err);
            }
        };

        for dev in devices {
            if dev.index == id {
                return Ok(dev.device);
            }
        }
        Err("Device not found".to_string())
    }

    fn get_default_device() -> Result<IMMDevice, String> {
        unsafe {
            let enumerator: IMMDeviceEnumerator =
                match CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL) {
                    Ok(device_enumerator) => device_enumerator,
                    Err(err) => {
                        return Err(format!("Error getting device enumerator: {}", err));
                    }
                };

            match enumerator.GetDefaultAudioEndpoint(eRender, eMultimedia) {
                Ok(device) => Ok(device),
                Err(err) => Err(format!("Error getting default device {}", err)),
            }
        }
    }

    pub fn get_capabilities(&self) -> Result<(), String> {
        unsafe {
            let device = Self::get_device(self.index)?;

            let audio_client: IAudioClient = match device.Activate(CLSCTX_ALL, None) {
                Ok(audio_client) => audio_client,
                Err(err) => {
                    return Err(format!("Error activating audio client: {}", err));
                }
            };

            let wave_format = Stream::create_waveformat_from(StreamParams {
                channels: 2,
                samplerate: crate::audio::SampleRate::Rate44100Hz,
                bits_per_sample: crate::audio::BitsPerSample::Bits16,
                buffer_length: 0,
                exclusive: true,
            });

            let sharemode = match true {
                true => AUDCLNT_SHAREMODE_EXCLUSIVE,
                false => AUDCLNT_SHAREMODE_SHARED,
            };
            match audio_client.IsFormatSupported(
                sharemode,
                &wave_format.Format as *const WAVEFORMATEX,
                None,
            ) {
                S_OK => true,
                result => {
                    return Err(format!(
                        "Error checking format support: {} - {}",
                        host_error(result),
                        "Unsuported format"
                    ));
                }
            };
        }
        Ok(())
    }
}
