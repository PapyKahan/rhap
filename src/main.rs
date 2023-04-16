use std::fs::File;
//use std::io::BufReader;
//use windows::Win32::Foundation::*;
use claxon::{Block, FlacReader};
use std::slice;
use std::ffi::OsString;
use std::os::windows::ffi::OsStringExt;
use windows::Win32::Media::KernelStreaming::{WAVE_FORMAT_EXTENSIBLE, KSDATAFORMAT_SUBTYPE_PCM};
use windows::Win32::System::Com::{CoInitialize, CoCreateInstance, CLSCTX_ALL, STGM_READ, VT_LPWSTR};
use windows::Win32::System::Com::StructuredStorage::PropVariantClear;
use windows::Win32::UI::Shell::PropertiesSystem::IPropertyStore;
use windows::Win32::Devices::FunctionDiscovery::*;
use windows::Win32::Media::Audio::*;

struct Device {
    inner_device : IMMDevice,
    id: u32,
    name: String,
}

fn enumerate_devices() -> Result<Vec<Device>, String> {
    let mut enumerated_devices = vec![];

    unsafe {
        // Initialise les sous-systèmes COM
        let _ = CoInitialize(None);

        let enumerator : IMMDeviceEnumerator = match CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL) {
            Ok(device_enumerator) => { device_enumerator },
            Err(err) => {
                println!("Error getting device enumerator: {}", err);
                return Err("Error getting device enumerator".to_string());
            },
        };

        //let d = enumerator.GetDefaultAudioEndpoint(eRender, eMultimedia).unwrap();

        let devices : IMMDeviceCollection = match enumerator.EnumAudioEndpoints(eRender, DEVICE_STATE_ACTIVE) {
            Ok(devices) => { devices },
            Err(err) => {
                println!("Error getting device list: {}", err);
                return Err("Error getting device list".to_string());
            },
        };

        for i in 0..devices.GetCount().unwrap() {
            let device : IMMDevice = match devices.Item(i) {
                Ok(device) => { device },
                Err(err) => {
                    println!("Error getting device: {}", err);
                    return Err("Error getting device".to_string());
                },
            };

            let property_store : IPropertyStore = device.OpenPropertyStore(STGM_READ).unwrap();
            let mut name_property_value = property_store.GetValue(&PKEY_Device_FriendlyName).unwrap();

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

            //let id = match device.GetId() {
            //    Ok(id) => { id },
            //    Err(err) => {
            //        println!("Error getting device id: {}", err);
            //        return Err("Error getting device id".to_string());
            //    },
            //};

            //println!("Device ID: {} : {}", id.to_string().unwrap(), name_string);
            let enumerated_device = Device {
                inner_device: device,
                id: i,
                name: name_string,
            };
            enumerated_devices.push(enumerated_device);
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
                println!("Device: id={}, name={}", dev.id, dev.name);
            }
            return Ok(());
        },
    };

    let mut device : *const Device = std::ptr::null();
    let devices = enumerate_devices().unwrap();
    let selected_device_id = 1;

    for dev in devices {
        if dev.id == selected_device_id {
            println!("Selected device: id={}, name={}", dev.id, dev.name);
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
        // Crée un périphérique audio WASAPI exclusif.

        let client = (*device).inner_device.Activate::<IAudioClient>(
            CLSCTX_ALL,
            None,
        ).unwrap();


        let format = client.GetMixFormat().unwrap();

        let wave_format : *mut WAVEFORMATEXTENSIBLE = format.clone() as *mut WAVEFORMATEXTENSIBLE;
        (*wave_format).Format.wFormatTag = WAVE_FORMAT_EXTENSIBLE as u16;
        (*wave_format).SubFormat = KSDATAFORMAT_SUBTYPE_PCM;

        (*wave_format).Format.wBitsPerSample = flac_reader.streaminfo().bits_per_sample as u16;
        (*wave_format).Format.nChannels = flac_reader.streaminfo().channels as u16;
        (*wave_format).Format.nSamplesPerSec = flac_reader.streaminfo().sample_rate as u32;

        (*wave_format).Format.nBlockAlign = (*wave_format).Format.nChannels * (*wave_format).Format.wBitsPerSample / 8;
        (*wave_format).Format.nAvgBytesPerSec = (*wave_format).Format.nSamplesPerSec * (*wave_format).Format.nBlockAlign as u32;

        //let event_handle = windows::synch::Event::create(None, true, false, None)?;
        let sharmode = AUDCLNT_SHAREMODE_EXCLUSIVE;
        let flags = AUDCLNT_STREAMFLAGS_NOPERSIST | AUDCLNT_STREAMFLAGS_AUTOCONVERTPCM;
        let result = client.Initialize(
            sharmode,
            flags,
            0,
            0,
            &(*wave_format).Format,
            None
        );

        if result.is_err() {
            println!("Error: {:?}", result);
            return Err(());
        }

        //client.set_event_handle(event_handle.handle)?;
        let result = client.Start();
        if result.is_err() {
            println!("Error: {:?}", result);
            return Err(());
        }

        let buffer_size = client.GetBufferSize().unwrap() as u32;
        let buffer_frame_count = buffer_size / (*wave_format).Format.nBlockAlign as u32;

        let mut frame_reader = flac_reader.blocks();
        let mut block = Block::empty();
        loop {
            let frame = block.into_buffer().clone();
            let frame_len = frame.len();
            match frame_reader.read_next_or_eof(frame) {
                Ok(Some(next_block)) => block = next_block,
                Ok(None) => break, // EOF.
                Err(error) => panic!("{}", error),
            };

            let padding = client.GetCurrentPadding().unwrap();
            let frames_available = buffer_frame_count - padding;

            let client_renderer = client.GetService::<IAudioRenderClient>().unwrap();
            let client_buffer = client_renderer.GetBuffer(frames_available).unwrap();
            let data = client_buffer as *mut ();

            //let frame : Vec<i32> = block.into_buffer();
            if frames_available >= frame.len() as u32 {
                let len = frames_available as usize * (*wave_format).Format.wBitsPerSample as usize / flac_reader.streaminfo().sample_rate as usize;
                std::slice::from_raw_parts(data as *const u8, len);
            } else {
                let frames_to_write = frames_available as usize;
                let frames_to_keep = frame.len() - frames_to_write;

                let len = frames_to_keep as usize * (*wave_format).Format.wBitsPerSample as usize / flac_reader.streaminfo().sample_rate as usize;
                std::slice::from_raw_parts(data as *const u8, len);
                // TODO : keep the rest of the frame
            }
        }
    } 
    return Ok(());
}
