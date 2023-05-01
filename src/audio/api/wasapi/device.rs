use super::host::Host;
use windows::{
    core::PCWSTR,
    Win32::System::Com::{CoInitializeEx, CoUninitialize, COINIT_MULTITHREADED},
};

pub struct Device {
    pub id: PCWSTR,
    pub index: u16,
    pub name: String,
}

impl Device {
    pub fn get_device(id: u16) -> Result<PCWSTR, String> {
        let mut selected_device: PCWSTR = PCWSTR(std::ptr::null_mut());

        let devices = match Host::enumerate_devices() {
            Ok(devices) => devices,
            Err(err) => {
                println!("Error enumerating devices: {}", err);
                return Err(err);
            }
        };

        for dev in devices {
            if dev.index == id {
                selected_device = dev.id;
                break;
            }
        }

        if selected_device.is_null() {
            println!("Device not found");
            return Err("Device not found".to_string());
        }

        Ok(selected_device)
    }

    pub fn get_capabilities(&self) -> Result<(), String> {
        unsafe {
            // Initialise les sous-systèmes COM
            match CoInitializeEx(None, COINIT_MULTITHREADED) {
                Ok(_) => (),
                Err(err) => {
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

    pub fn new(inner_device_id: PCWSTR, index: u16, name: String) -> Device {
        let this = Self {
            id: inner_device_id,
            index,
            name,
        };

        this
    }
}
