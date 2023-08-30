use std::{ffi::OsString, os::windows::prelude::OsStringExt, slice};
use widestring::U16CString;
use windows::Win32::{
    Devices::FunctionDiscovery::PKEY_Device_FriendlyName,
    Foundation::S_OK,
    Media::Audio::{
        eMultimedia, eRender, IAudioClient, IMMDevice, IMMDeviceEnumerator, MMDeviceEnumerator,
        AUDCLNT_SHAREMODE_EXCLUSIVE, AUDCLNT_SHAREMODE_SHARED, WAVEFORMATEX,
    },
    System::Com::{
        CoCreateInstance, StructuredStorage::{PropVariantClear, PropVariantToStringAlloc}, CLSCTX_ALL, STGM_READ
    },
    UI::Shell::PropertiesSystem::IPropertyStore,
};

use super::{host::Host, stream::Stream, utils::host_error, com::com_initialize};
use crate::audio::{DeviceTrait, StreamParams};

pub struct Device {
    pub index: u32,
    pub is_default: bool,
    pub(super) device: IMMDevice,
}

unsafe impl Send for Device {}
unsafe impl Sync for Device {}

impl DeviceTrait for Device {
    fn get_name(&self) -> String {
        match Self::get_device_name(&self.device) {
            Ok(name) => name,
            Err(_) => String::from(""),
        }
    }

    fn build_stream(&self, params: StreamParams) -> Result<Box<dyn crate::audio::StreamTrait>, String>
    {
        let stream = Stream::build_from_device(&self.device, params)?;
        Ok(Box::new(stream))
    }
}

impl Device {
    pub(super) fn new(id: Option<u32>) -> Result<Self, String> {
        let device = match id {
            Some(id) => Self::get_device(id)?,
            _ => Self::get_default_device()?,
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

            let propstring = PropVariantToStringAlloc(&name_property_value).unwrap();
            let wide_string = U16CString::from_ptr_str(propstring.0);
            let name = wide_string.to_string_lossy();

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
        let devices = Host::enumerate_devices()?;
        for dev in devices {
            if dev.index == id {
                return Ok(dev.device);
            }
        }
        Err(format!("Device id={} not found", id))
    }

    fn get_default_device() -> Result<IMMDevice, String> {
        unsafe {
            com_initialize();
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
