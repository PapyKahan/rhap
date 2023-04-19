//
// reference : Shared mode streaming : https://learn.microsoft.com/en-us/windows/win32/coreaudio/rendering-a-stream
// reference : Exclusive mode streaming : https://learn.microsoft.com/en-us/windows/win32/coreaudio/exclusive-mode-streams
// reference : https://www.hresult.info/FACILITY_AUDCLNT
//
use claxon::{Block, FlacReader};
use windows::Win32::Foundation::*;
use windows::Win32::System::Threading::{CreateEventW, WaitForSingleObject, CreateEventA};
use std::ffi::OsString;
use std::os::windows::ffi::OsStringExt;
use std::slice;
use windows::core::{PCWSTR, PWSTR, HRESULT};
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

fn host_error<'life>(errorcode: HRESULT) ->  &'life str {
    match errorcode {
        S_OK => "S_OK",
        E_POINTER => "E_POINTER",
        E_INVALIDARG => "E_INVALIDARG",
        AUDCLNT_E_NOT_INITIALIZED => "AUDCLNT_E_NOT_INITIALIZED",
        AUDCLNT_E_ALREADY_INITIALIZED => "AUDCLNT_E_ALREADY_INITIALIZED",
        AUDCLNT_E_WRONG_ENDPOINT_TYPE => "AUDCLNT_E_WRONG_ENDPOINT_TYPE",
        AUDCLNT_E_DEVICE_INVALIDATED => "AUDCLNT_E_DEVICE_INVALIDATED",
        AUDCLNT_E_NOT_STOPPED => "AUDCLNT_E_NOT_STOPPED",
        AUDCLNT_E_BUFFER_TOO_LARGE => "AUDCLNT_E_BUFFER_TOO_LARGE",
        AUDCLNT_E_OUT_OF_ORDER => "AUDCLNT_E_OUT_OF_ORDER",
        AUDCLNT_E_UNSUPPORTED_FORMAT => "AUDCLNT_E_UNSUPPORTED_FORMAT",
        AUDCLNT_E_INVALID_SIZE => "AUDCLNT_E_INVALID_SIZE",
        AUDCLNT_E_DEVICE_IN_USE => "AUDCLNT_E_DEVICE_IN_USE",
        AUDCLNT_E_BUFFER_OPERATION_PENDING => "AUDCLNT_E_BUFFER_OPERATION_PENDING",
        AUDCLNT_E_THREAD_NOT_REGISTERED => "AUDCLNT_E_THREAD_NOT_REGISTERED",
        AUDCLNT_E_EXCLUSIVE_MODE_NOT_ALLOWED => "AUDCLNT_E_EXCLUSIVE_MODE_NOT_ALLOWED",
        AUDCLNT_E_ENDPOINT_CREATE_FAILED => "AUDCLNT_E_ENDPOINT_CREATE_FAILED",
        AUDCLNT_E_SERVICE_NOT_RUNNING => "AUDCLNT_E_SERVICE_NOT_RUNNING",
        AUDCLNT_E_EVENTHANDLE_NOT_EXPECTED => "AUDCLNT_E_EVENTHANDLE_NOT_EXPECTED",
        AUDCLNT_E_EXCLUSIVE_MODE_ONLY => "AUDCLNT_E_EXCLUSIVE_MODE_ONLY",
        AUDCLNT_E_BUFDURATION_PERIOD_NOT_EQUAL => "AUDCLNT_E_BUFDURATION_PERIOD_NOT_EQUAL",
        AUDCLNT_E_EVENTHANDLE_NOT_SET => "AUDCLNT_E_EVENTHANDLE_NOT_SET",
        AUDCLNT_E_INCORRECT_BUFFER_SIZE => "AUDCLNT_E_INCORRECT_BUFFER_SIZE",
        AUDCLNT_E_BUFFER_SIZE_ERROR => "AUDCLNT_E_BUFFER_SIZE_ERROR",
        AUDCLNT_E_CPUUSAGE_EXCEEDED => "AUDCLNT_E_CPUUSAGE_EXCEEDED",
        AUDCLNT_E_BUFFER_ERROR => "AUDCLNT_E_BUFFER_ERROR",
        AUDCLNT_E_BUFFER_SIZE_NOT_ALIGNED => "AUDCLNT_E_BUFFER_SIZE_NOT_ALIGNED",
        AUDCLNT_E_INVALID_DEVICE_PERIOD => "AUDCLNT_E_INVALID_DEVICE_PERIOD",
        AUDCLNT_E_INVALID_STREAM_FLAG => "AUDCLNT_E_INVALID_STREAM_FLAG",
        AUDCLNT_E_ENDPOINT_OFFLOAD_NOT_CAPABLE => "AUDCLNT_E_ENDPOINT_OFFLOAD_NOT_CAPABLE",
        AUDCLNT_E_OUT_OF_OFFLOAD_RESOURCES => "AUDCLNT_E_OUT_OF_OFFLOAD_RESOURCES",
        AUDCLNT_E_OFFLOAD_MODE_ONLY => "AUDCLNT_E_OFFLOAD_MODE_ONLY",
        AUDCLNT_E_NONOFFLOAD_MODE_ONLY => "AUDCLNT_E_NONOFFLOAD_MODE_ONLY",
        AUDCLNT_E_RESOURCES_INVALIDATED => "AUDCLNT_E_RESOURCES_INVALIDATED",
        AUDCLNT_E_RAW_MODE_UNSUPPORTED => "AUDCLNT_E_RAW_MODE_UNSUPPORTED",
        AUDCLNT_E_ENGINE_PERIODICITY_LOCKED => "AUDCLNT_E_ENGINE_PERIODICITY_LOCKED",
        AUDCLNT_E_ENGINE_FORMAT_LOCKED => "AUDCLNT_E_ENGINE_FORMAT_LOCKED",
        AUDCLNT_S_BUFFER_EMPTY => "AUDCLNT_S_BUFFER_EMPTY",
        AUDCLNT_S_THREAD_ALREADY_REGISTERED => "AUDCLNT_S_THREAD_ALREADY_REGISTERED",
        AUDCLNT_S_POSITION_STALLED => "AUDCLNT_S_POSITION_STALLED",
        _ => "Unknown error",
    }
}

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

// Aligns 'v' backwards
fn align_bwd(v : u32, align : u32) -> u32
{
    // (v - (align ? v % align : 0))
    if align != 0 {
        v - (v % align)
    }
    else {
        v
    }
}

fn calculate_periodicity(
    sharemode : AUDCLNT_SHAREMODE,
    streamflags : AUDCLNT_STREAMOPTIONS,
    framesperbuffer : u32,
    waveformat : WAVEFORMATEX,
    periodicity : u32) {


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

    // Lit le fichier FLAC et écrit les échantillons dans le périphérique audio.
    let mut flac_reader = FlacReader::open(&file_path).expect("Failed to open FLAC file");

    unsafe {
        match CoInitialize(None) {
            Ok(_) => (),
            Err(err) => {
                println!("Error initializing COM: {} - {}", host_error(err.code()), err);
                return Err(());
            }
        }
        let enumerator: IMMDeviceEnumerator =
            match CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL) {
                Ok(device_enumerator) => device_enumerator,
                Err(err) => {
                    println!("Error getting device enumerator: {} - {}", host_error(err.code()), err);
                    return Err(());
                }
            };

        let d = match enumerator.GetDevice(PCWSTR((*device).inner_device_id.as_ptr())) {
            Ok(device) => device,
            Err(err) => {
                println!("Error getting device: {} - {}", host_error(err.code()), err);
                return Err(());
            }
        };

        // Crée un périphérique audio WASAPI exclusif.
        let client = match d.Activate::<IAudioClient>(CLSCTX_ALL, None) {
            Ok(client) => client,
            Err(err) => {
                println!("Error activating device: {} - {}", host_error(err.code()), err);
                return Err(());
            }
        };

        let wave_format = match client.GetMixFormat() {
            Ok(wave_format) => wave_format,
            Err(err) => {
                println!("Error getting mix format: {} - {}", host_error(err.code()), err);
                return Err(());
            }
        };
        
        let formattag = WAVE_FORMAT_PCM;
        let channels = flac_reader.streaminfo().channels as u32;
        let sample_rate = flac_reader.streaminfo().sample_rate as u32;
        let bits_per_sample = flac_reader.streaminfo().bits_per_sample as u32;
        let block_align = channels * bits_per_sample / 8;
        let bytes_per_second = sample_rate * block_align;

        // WAVEFORMATEX documentation: https://learn.microsoft.com/en-us/windows/win32/api/mmreg/ns-mmreg-waveformatex
        (*wave_format).wFormatTag = formattag as u16;
        (*wave_format).nChannels = channels as u16;
        (*wave_format).nSamplesPerSec = sample_rate;
        (*wave_format).wBitsPerSample = bits_per_sample as u16;
        (*wave_format).nBlockAlign = block_align as u16;
        (*wave_format).nAvgBytesPerSec = bytes_per_second;
        (*wave_format).cbSize = 0;

        println!("nChannels: {}", channels);
        println!("nSamplesPerSec: {}", sample_rate);
        println!("wBitsPerSample: {}", bits_per_sample);
        println!("nBlockAlign: {}", block_align);
        println!("nAvgBytesPerSec: {}", bytes_per_second);


        // WAVEFORMATEXTENSIBLE documentation: https://docs.microsoft.com/en-us/windows/win32/api/mmreg/ns-mmreg-waveformatextensible
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
                println!("Error getting device period: {} - {}", host_error(err.code()), err);
                return Err(());
            }
        };

        let streamflags = AUDCLNT_STREAMFLAGS_EVENTCALLBACK;
        let sharemode = AUDCLNT_SHAREMODE_EXCLUSIVE;
        let result = client.IsFormatSupported(sharemode, wave_format, None);
        if result != S_OK {
            println!("Format not supported: {} - {}", host_error(result), result);
            return Err(());
        }

        let result = client.Initialize(
            sharemode,
            streamflags,
            minimum_device_period,
            minimum_device_period,
            wave_format,
            None,
        );

        if result.is_err() {
            if result.as_ref().err().unwrap().code() != AUDCLNT_E_BUFFER_SIZE_NOT_ALIGNED {
                let err = result.err().unwrap();
                println!("Error initializing client: {} - {}", host_error(err.code()), err);
                return Err(());
            }
            println!("Buffer size not aligned");
            let buffer_size = match client.GetBufferSize() {
                Ok(buffer_size) => buffer_size,
                Err(err) => {
                    println!("Initialize: Error getting buffer size: {} - {}", host_error(err.code()), err);
                    return Err(());
                }
            };
            let buffer_size = buffer_size * block_align;
            let minimum_device_period  = REFTIMES_PER_SEC as u32 / sample_rate * buffer_size;

            println!("Minimum device period: REFTIMES_PER_SEC ({}) / nSamplesPerSec ({}) * buffer_size ({}) = {}", REFTIMES_PER_SEC, flac_reader.streaminfo().sample_rate, buffer_size, minimum_device_period);

            match client.Initialize(
                sharemode,
                streamflags,
                minimum_device_period as i64,
                minimum_device_period as i64,
                wave_format,
                None,
            ) {
                Ok(_) => (),
                Err(err) => {
                    println!("Error initializing client: {} - {}", host_error(err.code()), err);
                    return Err(());
                }
            }
        }

        let eventhandle = match CreateEventW(
            None,
            FALSE,
            FALSE,
            PCWSTR::null(),
        ) {
            Ok(eventhandle) => eventhandle,
            Err(err) => {
                println!("Error creating event handle: {} - {}", host_error(err.code()), err);
                return Err(());
            }
        };

        match client.SetEventHandle(eventhandle) {
            Ok(_) => (),
            Err(err) => {
                println!("Error setting event handle: {} - {}", host_error(err.code()), err);
                return Err(());
            }
        }

        let buffer_size = match client.GetBufferSize() {
            Ok(buffer_size) => buffer_size,
            Err(err) => {
                println!("Size: Error getting buffer size: {} - {}", host_error(err.code()), err);
                return Err(());
            }
        };


        let client_renderer = match client.GetService::<IAudioRenderClient>() {
            Ok(client_renderer) => client_renderer,
            Err(err) => {
                println!("Error getting client renderer: {} - {}", host_error(err.code()), err);
                return Err(());
            }
        };


        match client.Start() {
            Ok(_) => (),
            Err(err) => {
                println!("Error starting client: {} - {}", host_error(err.code()), err);
                return Err(());
            }
        }

        let mut frame_reader = flac_reader.blocks();
        let mut block = Block::empty();

        loop {
            match WaitForSingleObject(eventhandle, 2000) {
                WAIT_OBJECT_0 => (),
                WAIT_TIMEOUT => {
                    println!("Timeout");
                    break;
                },
                WAIT_FAILED => {
                    println!("Wait failed");
                    break;
                },
                _ => (),
            }

            match frame_reader.read_next_or_eof(block.into_buffer()) {
                Ok(Some(next_block)) => {
                    block = next_block;
                },
                Ok(None) => break, // EOF.
                Err(error) => panic!("{}", error),
            };

            let mut index = 0;

            let client_buffer = match client_renderer.GetBuffer(buffer_size) {
                Ok(client_buffer) => client_buffer,
                Err(err) => {
                    println!("Error getting client buffer: {} - {}", host_error(err.code()), err);
                    return Err(());
                }
            };

            let client_buffer_len = buffer_size as usize * (*wave_format).wBitsPerSample as usize;
            let data = std::slice::from_raw_parts_mut(client_buffer, client_buffer_len);
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
            match client_renderer.ReleaseBuffer(frames_writen, 0) {
                Ok(_) => (),
                Err(err) => {
                    println!("Error releasing client buffer: {} - {}", host_error(err.code()), err);
                    return Err(());
                }
            };

            if index > block.len() {
                break;
            }
        }

        match client.Stop() {
            Ok(_) => (),
            Err(err) => {
                println!("Error stopping client: {} - {}", host_error(err.code()), err);
                return Err(());
            }
        }
        println!("Done playing");
    }
    return Ok(());
}
