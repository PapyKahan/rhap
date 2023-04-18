//
// reference : Shared mode streaming : https://learn.microsoft.com/en-us/windows/win32/coreaudio/rendering-a-stream
// reference : Exclusive mode streaming : https://learn.microsoft.com/en-us/windows/win32/coreaudio/exclusive-mode-streams
//
use claxon::{Block, FlacReader};
use windows::Win32::Foundation::*;
use windows::Win32::System::Threading::{CreateEventW, WaitForSingleObject};
use std::ffi::OsString;
use std::mem::size_of;
use std::os::windows::ffi::OsStringExt;
use std::slice;
use windows::core::{PCWSTR, PWSTR};
use windows::Win32::Devices::FunctionDiscovery::*;
use windows::Win32::Media::Audio::*;
use windows::Win32::Media::KernelStreaming::{
    KSDATAFORMAT_SUBTYPE_PCM, SPEAKER_FRONT_LEFT, SPEAKER_FRONT_RIGHT, WAVE_FORMAT_EXTENSIBLE,
};
use windows::Win32::System::Com::StructuredStorage::PropVariantClear;
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitialize, CLSCTX_ALL, STGM_READ, VT_LPWSTR,
};
use windows::Win32::UI::Shell::PropertiesSystem::IPropertyStore;

const REFTIMES_PER_SEC : i64 = 10000000;
const REFTIMES_PER_MILLISEC : i64 = 10000;

struct Device {
    inner_device_id: PWSTR,
    index: u32,
    name: String,
}

impl Device {
    pub fn new(inner_device_id: PWSTR, index: u32, name: String) -> Device {
        Self {
            inner_device_id,
            index,
            name,
        }
    }
}

fn enumerate_devices() -> Result<Vec<Device>, String> {
    let mut enumerated_devices = vec![];

    unsafe {
        // Initialise les sous-systèmes COM
        let _ = CoInitialize(None);

        let enumerator: IMMDeviceEnumerator =
            match CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL) {
                Ok(device_enumerator) => device_enumerator,
                Err(err) => {
                    println!("Error getting device enumerator: {}", err);
                    return Err("Error getting device enumerator".to_string());
                }
            };

        //let d = enumerator.GetDefaultAudioEndpoint(eRender, eMultimedia).unwrap();

        let devices: IMMDeviceCollection =
            match enumerator.EnumAudioEndpoints(eRender, DEVICE_STATE_ACTIVE) {
                Ok(devices) => devices,
                Err(err) => {
                    println!("Error getting device list: {}", err);
                    return Err("Error getting device list".to_string());
                }
            };

        for i in 0..devices.GetCount().unwrap() {
            let device: IMMDevice = match devices.Item(i) {
                Ok(device) => device,
                Err(err) => {
                    println!("Error getting device: {}", err);
                    return Err("Error getting device".to_string());
                }
            };

            let property_store: IPropertyStore = device.OpenPropertyStore(STGM_READ).unwrap();
            let mut name_property_value =
                property_store.GetValue(&PKEY_Device_FriendlyName).unwrap();

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
            let name_string = match name_os_string.into_string() {
                Ok(string) => string,
                Err(os_string) => os_string.to_string_lossy().into(),
            };

            // Clean up the property.
            PropVariantClear(&mut name_property_value).ok();

            let id: PWSTR = match device.GetId() {
                Ok(id) => id,
                Err(err) => {
                    println!("Error getting device id: {}", err);
                    return Err("Error getting device id".to_string());
                }
            };

            enumerated_devices.push(Device::new(id, i, name_string));
        }

        Ok(enumerated_devices)
    }
}

fn main() -> Result<(), ()> {
    let args = std::env::args().collect::<Vec<String>>();

    let file_path = match args.len() {
        2 => &args[1],
        _ => {
            println!("Usage: rhap <file>");
            let devices = enumerate_devices().unwrap();
            for dev in devices {
                println!("Device: id={}, name={}", dev.index, dev.name);
            }
            return Ok(());
        }
    };

    let mut device: *const Device = std::ptr::null();
    let devices = enumerate_devices().unwrap();
    let selected_device_id = 0;

    for dev in devices {
        if dev.index == selected_device_id {
            println!("Selected device: id={}, name={}", dev.index, dev.name);
            device = &dev;
        }
    }

    if device.is_null() {
        println!("Device not found");
        return Err(());
    }

    if file_path.is_empty() {
        return Err(());
    }

    //let file = match File::open(file_path) {
    //    Ok(file) => file,
    //    Err(err) => {
    //        println!("Error opening file: {}", err);
    //        return Err(());
    //    },
    //};

    //// Load a sound from a file, using a path relative to Cargo.toml
    //let buffer = BufReader::new(file);

    // Lit le fichier FLAC et écrit les échantillons dans le périphérique audio.
    let mut flac_reader = FlacReader::open(&file_path).expect("Failed to open FLAC file");

    unsafe {
        let _ = CoInitialize(None);
        let enumerator: IMMDeviceEnumerator =
            match CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL) {
                Ok(device_enumerator) => device_enumerator,
                Err(err) => {
                    println!("Error getting device enumerator: {}", err);
                    return Err(());
                }
            };

        let d = match enumerator.GetDevice(PCWSTR((*device).inner_device_id.as_ptr())) {
            Ok(device) => device,
            Err(err) => {
                println!("Error getting device: {}", err);
                return Err(());
            }
        };

        // Crée un périphérique audio WASAPI exclusif.
        let client = match d.Activate::<IAudioClient>(CLSCTX_ALL, None) {
            Ok(client) => client,
            Err(err) => {
                println!("Error activating device: {}", err);
                return Err(());
            }
        };

        let wave_format = client.GetMixFormat().unwrap();
        (*wave_format).wFormatTag = WAVE_FORMAT_PCM as u16;
        //(*wave_format).wFormatTag = WAVE_FORMAT_EXTENSIBLE as u16;
        (*wave_format).nChannels = flac_reader.streaminfo().channels as u16;
        (*wave_format).nSamplesPerSec = flac_reader.streaminfo().sample_rate as u32;
        (*wave_format).wBitsPerSample = flac_reader.streaminfo().bits_per_sample as u16;
        (*wave_format).nBlockAlign = (*wave_format).nChannels * (*wave_format).wBitsPerSample / 8;
        (*wave_format).nAvgBytesPerSec = (*wave_format).nSamplesPerSec * (*wave_format).nBlockAlign as u32;
        (*wave_format).cbSize = size_of::<WAVEFORMATEX>() as u16;

        //let wave_format: *mut WAVEFORMATEXTENSIBLE = format.clone() as *mut WAVEFORMATEXTENSIBLE;
        //(*wave_format).Format.wFormatTag = WAVE_FORMAT_EXTENSIBLE as u16;
        //(*wave_format).Format.nChannels = flac_reader.streaminfo().channels as u16;
        //(*wave_format).Format.nSamplesPerSec = flac_reader.streaminfo().sample_rate as u32;
        //(*wave_format).Format.wBitsPerSample = flac_reader.streaminfo().bits_per_sample as u16;
        //(*wave_format).Format.nBlockAlign = (*wave_format).Format.nChannels * (*wave_format).Format.wBitsPerSample / 8;
        //(*wave_format).Format.nAvgBytesPerSec = (*wave_format).Format.nSamplesPerSec * (*wave_format).Format.nBlockAlign as u32;
        //(*wave_format).Format.cbSize = size_of::<WAVEFORMATEXTENSIBLE>() as u16 - size_of::<WAVEFORMATEX>() as u16;
        //(*wave_format).Samples.wValidBitsPerSample = (*wave_format).Format.wBitsPerSample;
        //(*wave_format).SubFormat = KSDATAFORMAT_SUBTYPE_PCM;
        //(*wave_format).dwChannelMask = SPEAKER_FRONT_LEFT | SPEAKER_FRONT_RIGHT;

        // Création des pointeurs pour les paramètres
        let mut default_device_period: i64 = 0;
        let mut minimum_device_period: i64 = 0;

        match client.GetDevicePeriod(Some(&mut default_device_period as *mut i64), Some(&mut minimum_device_period as *mut i64)) {
            Ok(_) => (),
            Err(err) => {
                println!("Error getting device period: {}", err);
                return Err(());
            }
        };

        //let flags = AUDCLNT_STREAMFLAGS_EVENTCALLBACK;
        let flags = 0;

        let result = client.Initialize(
            AUDCLNT_SHAREMODE_EXCLUSIVE,
            flags,
            minimum_device_period,
            minimum_device_period,
            wave_format,
            None,
        );

        if result.is_err() && result.err().unwrap().code() == AUDCLNT_E_BUFFER_SIZE_NOT_ALIGNED {
            println!("Buffer size not aligned");
            let buffer_size = match client.GetBufferSize() {
                Ok(buffer_size) => buffer_size as i64,
                Err(err) => {
                    println!("Initialize: Error getting buffer size: {}", err);
                    return Err(());
                }
            };
            let minimum_device_period  = REFTIMES_PER_SEC / (*wave_format).nSamplesPerSec as i64 * buffer_size;
            match client.Initialize(
                AUDCLNT_SHAREMODE_EXCLUSIVE,
                flags,
                minimum_device_period,
                minimum_device_period,
                wave_format,
                None,
            ) {
                Ok(_) => (),
                Err(err) => {
                    println!("Error initializing client: {}", err);
                    return Err(());
                }
            }
        }

        //let eventhandle = match CreateEventW(
        //    None,
        //    FALSE,
        //    FALSE,
        //    PCWSTR::null(),
        //) {
        //    Ok(eventhandle) => eventhandle,
        //    Err(err) => {
        //        println!("Error creating event handle: {}", err);
        //        return Err(());
        //    }
        //};

        //match client.SetEventHandle(eventhandle) {
        //    Ok(_) => (),
        //    Err(err) => {
        //        println!("Error setting event handle: {}", err);
        //        return Err(());
        //    }
        //}

        let buffer_size = match client.GetBufferSize() {
            Ok(buffer_size) => buffer_size,
            Err(err) => {
                println!("Size: Error getting buffer size: {}", err);
                return Err(());
            }
        };

        let client_renderer = match client.GetService::<IAudioRenderClient>() {
            Ok(client_renderer) => client_renderer,
            Err(err) => {
                println!("Error getting client renderer: {}", err);
                return Err(());
            }
        };

        match client.Start() {
            Ok(_) => (),
            Err(err) => {
                println!("Error starting client: {}", err);
                return Err(());
            }
        }

        let mut frame_reader = flac_reader.blocks();
        let mut block = Block::empty();

        loop {
            //match WaitForSingleObject(eventhandle, 2000) {
            //    WAIT_OBJECT_0 => (),
            //    WAIT_TIMEOUT => {
            //        println!("Timeout");
            //        break;
            //    },
            //    WAIT_FAILED => {
            //        println!("Wait failed");
            //        break;
            //    },
            //    _ => (),
            //}
            match frame_reader.read_next_or_eof(block.into_buffer()) {
                Ok(Some(next_block)) => {
                    block = next_block;
                },
                Ok(None) => break, // EOF.
                Err(error) => panic!("{}", error),
            };

            let mut index = 0;
            let client_buffer = client_renderer.GetBuffer(buffer_size).unwrap() as *mut ();
            let client_buffer_len = buffer_size as usize * (*wave_format).wBitsPerSample as usize;
            let data = std::slice::from_raw_parts_mut(client_buffer as *mut u8, client_buffer_len);
            let mut frames_writen = 0;

            for i in 0..buffer_size as usize {
                let left_channel_sample = block.sample(0, index);
                let left_channel_sample = left_channel_sample.to_le_bytes();
                let right_channel_sample = block.sample(1, index);
                let right_channel_sample = right_channel_sample.to_le_bytes();

                for j in 0..left_channel_sample.len() {
                    data[i * (*wave_format).nBlockAlign as usize + 0 as usize] = left_channel_sample[j];
                    data[i * (*wave_format).nBlockAlign as usize + 2 as usize] = right_channel_sample[j];
                }
                frames_writen += 1;
                index += 1;
                if index > block.len() {
                    break;
                }
            }

            println!("frames writen: {}", frames_writen);
            client_renderer.ReleaseBuffer(frames_writen, 0).unwrap();

            if index > block.len() {
                break;
            }
        }

        match client.Stop() {
            Ok(_) => (),
            Err(err) => {
                println!("Error stopping client: {}", err);
                return Err(());
            }
        }
        println!("Done playing");
    }
    return Ok(());
}
