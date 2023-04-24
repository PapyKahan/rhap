//
// TODO add commandline parsing : https://docs.rs/clap/latest/clap/
// reference : Shared mode streaming : https://learn.microsoft.com/en-us/windows/win32/coreaudio/rendering-a-stream
// reference : Exclusive mode streaming : https://learn.microsoft.com/en-us/windows/win32/coreaudio/exclusive-mode-streams
// reference : https://www.hresult.info/FACILITY_AUDCLNT
//
use claxon::{Block, FlacReader};
use std::collections::VecDeque;
use std::ffi::OsString;
use std::mem::size_of;
use std::os::windows::ffi::OsStringExt;
use std::slice;
use windows::core::PCWSTR;
use windows::Win32::Devices::FunctionDiscovery::*;
use windows::Win32::Foundation::*;
use windows::Win32::Media::Audio::*;
use windows::Win32::Media::KernelStreaming::{
    KSDATAFORMAT_SUBTYPE_PCM, SPEAKER_FRONT_LEFT, SPEAKER_FRONT_RIGHT, WAVE_FORMAT_EXTENSIBLE,
};
use windows::Win32::System::Com::StructuredStorage::PropVariantClear;
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CLSCTX_ALL, COINIT_MULTITHREADED, STGM_READ, VT_LPWSTR,
};
use windows::Win32::System::Threading::{CreateEventW, WaitForSingleObject};
use windows::Win32::UI::Shell::PropertiesSystem::IPropertyStore;

use crate::audio::api::wasapi::utils::{host_error, print_wave_format};

mod audio;

const REFTIMES_PER_SEC: i64 = 10000000;
//const REFTIMES_PER_MILLISEC : i64 = 10000;

struct Device {
    id: PCWSTR,
    index: u32,
    name: String,
}

impl Device {
    pub fn new(inner_device_id: PCWSTR, index: u32, name: String) -> Device {
        Self {
            id: inner_device_id,
            index,
            name,
        }
    }
}

fn enumerate_devices() -> Result<Vec<Device>, String> {
    let mut enumerated_devices = vec![];

    unsafe {
        // Initialise les sous-systèmes COM
        match CoInitializeEx(None, COINIT_MULTITHREADED) {
            Ok(_) => (),
            Err(err) => {
                println!("Error initialising COM: {}", err);
                return Err("Error initialising COM".to_string());
            }
        }

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

            enumerated_devices.push(Device::new(id, index, name));
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
                unsafe {
                    println!(
                        "Device: id={}, name={}, inner_id={}",
                        dev.index,
                        dev.name,
                        dev.id.display().to_string()
                    );
                }
            }
            return Ok(());
        }
    };

    let selected_device_id = 0;
    let devices = match enumerate_devices() {
        Ok(devices) => devices,
        Err(err) => {
            println!("Error enumerating devices: {}", err);
            return Err(());
        }
    };

    let mut selected_device: *const Device = std::ptr::null();
    for dev in devices {
        if dev.index == selected_device_id {
            println!("Selected device: id={}, name={}", dev.index, dev.name);
            selected_device = &dev;
            break;
        }
    }

    if selected_device.is_null() {
        println!("Device not found");
        return Err(());
    }

    if file_path.is_empty() {
        return Err(());
    }

    let mut flac_reader = FlacReader::open(&file_path).expect("Failed to open FLAC file");

    unsafe {
        match CoInitializeEx(None, COINIT_MULTITHREADED) {
            Ok(_) => (),
            Err(err) => {
                println!(
                    "Error initializing COM: {} - {}",
                    host_error(err.code()),
                    err
                );
                return Err(());
            }
        }

        let enumerator: IMMDeviceEnumerator =
            match CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL) {
                Ok(device_enumerator) => device_enumerator,
                Err(err) => {
                    println!(
                        "Error getting device enumerator: {} - {}",
                        host_error(err.code()),
                        err
                    );
                    return Err(());
                }
            };

        let device = match enumerator.GetDevice((*selected_device).id) {
            Ok(device) => device,
            Err(err) => {
                println!(
                    "Error getting device: {} - {}",
                    host_error(err.code()),
                    err
                );
                return Err(());
            }
        };

        // Crée un périphérique audio WASAPI exclusif.
        let client = match device.Activate::<IAudioClient>(CLSCTX_ALL, None) {
            Ok(client) => client,
            Err(err) => {
                println!(
                    "Error activating device: {} - {}",
                    host_error(err.code()),
                    err
                );
                return Err(());
            }
        };

        //let wave_format = match client.GetMixFormat() {
        //    Ok(wave_format) => wave_format,
        //    Err(err) => {
        //        println!("Error getting mix format: {} - {}", audio::log::host_error(err.code()), err);
        //        return Err(());
        //    }
        //};

        let formattag = WAVE_FORMAT_EXTENSIBLE;
        let channels = flac_reader.streaminfo().channels as u32;
        let sample_rate = flac_reader.streaminfo().sample_rate as u32;
        let bits_per_sample = flac_reader.streaminfo().bits_per_sample as u32;
        let block_align = channels * bits_per_sample as u32 / 8;
        let bytes_per_second = sample_rate * block_align;

        // WAVEFORMATEX documentation: https://learn.microsoft.com/en-us/windows/win32/api/mmreg/ns-mmreg-waveformatex
        // WAVEFORMATEXTENSIBLE documentation: https://docs.microsoft.com/en-us/windows/win32/api/mmreg/ns-mmreg-waveformatextensible
        let wave_format: *const WAVEFORMATEXTENSIBLE = &mut WAVEFORMATEXTENSIBLE {
            Format: WAVEFORMATEX {
                wFormatTag: formattag as u16,
                nChannels: channels as u16,
                nSamplesPerSec: sample_rate,
                wBitsPerSample: bits_per_sample as u16,
                nBlockAlign: block_align as u16,
                nAvgBytesPerSec: bytes_per_second,
                cbSize: size_of::<WAVEFORMATEXTENSIBLE>() as u16 - size_of::<WAVEFORMATEX>() as u16,
            },
            Samples: WAVEFORMATEXTENSIBLE_0 {
                wValidBitsPerSample: bits_per_sample as u16,
            },
            dwChannelMask: SPEAKER_FRONT_LEFT | SPEAKER_FRONT_RIGHT,
            SubFormat: KSDATAFORMAT_SUBTYPE_PCM,
        };

        println!("--------------------------------------------------------------------------------------");
        print_wave_format(wave_format as *const WAVEFORMATEX);
        println!("--------------------------------------------------------------------------------------");

        let sharemode = AUDCLNT_SHAREMODE_EXCLUSIVE;
        let streamflags = AUDCLNT_STREAMFLAGS_EVENTCALLBACK;
        match client.IsFormatSupported(sharemode, wave_format as *const WAVEFORMATEX, None) {
            S_OK => (),
            result => {
                println!(
                    "Error checking format support: {} - {}",
                    host_error(result),
                    "Unsuporrted format"
                );
                return Err(());
            }
        };

        // Création des pointeurs pour les paramètres
        let mut default_device_period: i64 = 0;
        let mut minimum_device_period: i64 = 0;
        match client.GetDevicePeriod(
            Some(&mut default_device_period as *mut i64),
            Some(&mut minimum_device_period as *mut i64),
        ) {
            Ok(_) => (),
            Err(err) => {
                println!(
                    "Error getting device period: {} - {}",
                    host_error(err.code()),
                    err
                );
                return Err(());
            }
        };

        let result = client.Initialize(
            sharemode,
            streamflags,
            minimum_device_period,
            minimum_device_period,
            wave_format as *const WAVEFORMATEX,
            None,
        );

        if result.is_err() {
            if result.as_ref().err().unwrap().code() != AUDCLNT_E_BUFFER_SIZE_NOT_ALIGNED {
                let err = result.err().unwrap();
                println!(
                    "Error initializing client: {} - {}",
                    host_error(err.code()),
                    err
                );
                return Err(());
            }
            println!("Buffer size not aligned");
            let buffer_size = match client.GetBufferSize() {
                Ok(buffer_size) => buffer_size as i64,
                Err(err) => {
                    println!("Initialize: Error getting buffer size: {}", err);
                    return Err(());
                }
            };
            let minimum_device_period = REFTIMES_PER_SEC / sample_rate as i64 * buffer_size;
            match client.Initialize(
                sharemode,
                streamflags,
                minimum_device_period,
                minimum_device_period,
                wave_format as *const WAVEFORMATEX,
                None,
            ) {
                Ok(_) => (),
                Err(err) => {
                    println!("Error initializing client: {}", err);
                    return Err(());
                }
            }
        }

        let eventhandle = match CreateEventW(None, FALSE, FALSE, PCWSTR::null()) {
            Ok(eventhandle) => eventhandle,
            Err(err) => {
                println!(
                    "Error creating event handle: {} - {}",
                    host_error(err.code()),
                    err
                );
                return Err(());
            }
        };

        match client.SetEventHandle(eventhandle) {
            Ok(_) => (),
            Err(err) => {
                println!(
                    "Error setting event handle: {} - {}",
                    host_error(err.code()),
                    err
                );
                return Err(());
            }
        }

        let buffer_size = match client.GetBufferSize() {
            Ok(buffer_size) => buffer_size,
            Err(err) => {
                println!(
                    "Size: Error getting buffer size: {} - {}",
                    host_error(err.code()),
                    err
                );
                return Err(());
            }
        };

        let client_renderer = match client.GetService::<IAudioRenderClient>() {
            Ok(client_renderer) => client_renderer,
            Err(err) => {
                println!(
                    "Error getting client renderer: {} - {}",
                    host_error(err.code()),
                    err
                );
                return Err(());
            }
        };

        match client.Start() {
            Ok(_) => (),
            Err(err) => {
                println!(
                    "Error starting client: {} - {}",
                    host_error(err.code()),
                    err
                );
                return Err(());
            }
        }

        let mut frame_reader = flac_reader.blocks();
        let mut block = Block::empty();
        let mut vec_buffer = VecDeque::new();
        let bytes = block_align / channels;
        loop {
            match frame_reader.read_next_or_eof(block.into_buffer()) {
                Ok(Some(next_block)) => {
                    block = next_block;
                }
                Ok(None) => break, // EOF.
                Err(error) => panic!("{}", error),
            };

            for samples in block.stereo_samples() {
                let left = samples.0.to_le_bytes();
                let mut copied_bytes = 0;
                for l in left.iter() {
                    vec_buffer.push_back(*l);
                    copied_bytes += 1;
                    if copied_bytes >= bytes {
                        break;
                    }
                }
                let right = samples.1.to_le_bytes();
                copied_bytes = 0;
                for r in right.iter() {
                    vec_buffer.push_back(*r);
                    copied_bytes += 1;
                    if copied_bytes >= bytes {
                        break;
                    }
                }
            }
        }

        println!("Playing file path: {}", file_path);
        while vec_buffer.len() > 0 {
            match WaitForSingleObject(eventhandle, 2000) {
                WAIT_OBJECT_0 => (),
                WAIT_TIMEOUT => {
                    println!("Timeout");
                    break;
                }
                WAIT_FAILED => {
                    println!("Wait failed");
                    break;
                }
                _ => (),
            }

            let client_buffer = match client_renderer.GetBuffer(buffer_size) {
                Ok(buffer) => buffer,
                Err(err) => {
                    println!("Error getting client buffer: {}", err);
                    return Err(());
                }
            };

            // Compute client buffer size in bytes.
            let client_buffer_len =
                buffer_size as usize * (bits_per_sample / 8) as usize
                * channels as usize;
            // Convert client buffer to a slice of bytes.
            let data = std::slice::from_raw_parts_mut(client_buffer, client_buffer_len);

            for i in 0..client_buffer_len {
                if vec_buffer.len() == 0 {
                    break;
                }
                data[i] = vec_buffer.pop_front().unwrap();
            }

            match client_renderer.ReleaseBuffer(buffer_size, 0) {
                Ok(_) => (),
                Err(err) => {
                    println!(
                        "Error releasing client buffer: {} - {}",
                        host_error(err.code()),
                        err
                    );
                    return Err(());
                }
            };
        }

        match client.Stop() {
            Ok(_) => (),
            Err(err) => {
                println!(
                    "Error stopping client: {} - {}",
                    host_error(err.code()),
                    err
                );
                return Err(());
            }
        }
        println!("Done playing");
    }
    return Ok(());
}