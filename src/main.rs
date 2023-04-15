use windows::Devices::Enumeration::DeviceInformation;
use windows::Media::Audio::AudioDeviceOutputNode;
//use std::fs::File;
//use std::io::BufReader;
//use windows::Win32::Foundation::*;
use windows::Win32::System::Com::{CoInitialize, CoCreateInstance, CLSCTX_ALL};
use windows::Win32::Media::Audio::*;
//use windows::win32::windows_media_devices::{AUDIO_CLIENT_SHAREMODE_EXCLUSIVE, AUDCLNT_STREAMFLAGS_EVENTCALLBACK};
//use windows::win32::direct3d11::D3D_DRIVER_TYPE_NULL;
//use windows::win32::direct3d9::D3D_SDK_VERSION;
//use windows::win32::mmdeviceapi::{eRender, CLSCTX_ALL};
//use windows::win32::com::{CoInitializeEx, COINIT_APARTMENTTHREADED};

fn enumerate_devices() {
    unsafe {
        // Initialise les sous-systèmes COM et WinRT.
        let _ = CoInitialize(None);

        let enumerator : IMMDeviceEnumerator = match CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL) {
            Ok(device_enumerator) => { device_enumerator },
            Err(err) => {
                println!("Error getting device enumerator: {}", err);
                return;
            },
        };


        let d = enumerator.GetDefaultAudioEndpoint(eRender, eMultimedia).unwrap();

        let devices : IMMDeviceCollection = match enumerator.EnumAudioEndpoints(eRender, DEVICE_STATE_ACTIVE) {
            Ok(devices) => { devices },
            Err(err) => {
                println!("Error getting device list: {}", err);
                return;
            },
        };

        for i in 0..devices.GetCount().unwrap() {
            let device : IMMDevice = match devices.Item(i) {
                Ok(device) => { device },
                Err(err) => {
                    println!("Error getting device: {}", err);
                    return;
                },
            };

            let id : windows::core::PWSTR = match device.GetId() {
                Ok(id) => { id },
                Err(err) => {
                    println!("Error getting device id: {}", err);
                    return;
                },
            };

            println!("Device ID: {}", id.to_string().unwrap());
        }
    }
}

fn main() -> Result<(), ()> {
    //let args = std::env::args().collect::<Vec<String>>();

    //let file_path = match args.len() {
    //    2 => &args[1],
    //    _ => {
    //        println!("Usage: rhap <file>");
    //        enumerate_devices();
    //        ""
    //    },
    //};

    //if file_path.is_empty() {
    //    return;
    //}

    //let file = match File::open(file_path) {
    //    Ok(file) => file,
    //    Err(err) => {
    //        println!("Error opening file: {}", err);
    //        return;
    //    },
    //};

    //// Load a sound from a file, using a path relative to Cargo.toml
    //let buffer = BufReader::new(file);
    
    enumerate_devices();

    //unsafe {
        //let device = windows:Win32::Media::Audio::AudioDevice::get_default_audio_endpoint(eRender, windows::media::AUDIO_DEVICE_ROLE::Multimedia);

        //// Crée un périphérique audio WASAPI exclusif.
        //let device = windows::media::audio::AudioDevice::get_default_audio_endpoint(eRender, windows::media::AUDIO_DEVICE_ROLE::Multimedia)?;

        //let client = device.activate_audio_client()?;
        //let format = client.get_mix_format()?;
        //let mut wave_format = format.clone().into();
        //wave_format.wFormatTag = windows::win32::mmreg::WAVE_FORMAT_EXTENSIBLE as u16;
        //wave_format.SubFormat = windows::win32::ksmedia::KSDATAFORMAT_SUBTYPE_PCM;
        //wave_format.Format.wBitsPerSample = 16;
        //wave_format.Format.nBlockAlign = wave_format.Format.nChannels * wave_format.Format.wBitsPerSample / 8;
        //wave_format.Format.nAvgBytesPerSec = wave_format.Format.nSamplesPerSec * wave_format.Format.nBlockAlign;
        //let event_handle = windows::synch::Event::create(None, true, false, None)?;
        //client.initialize(AUDIO_CLIENT_SHAREMODE_EXCLUSIVE, AUDCLNT_STREAMFLAGS_EVENTCALLBACK, 0, 0, &wave_format.Format, std::ptr::null())?;
        //client.set_event_handle(event_handle.handle)?;

        //// Lit le fichier FLAC et écrit les échantillons dans le périphérique audio.
        //let flac_reader = claxon::FlacReader::new(file_handle)?;
        //let sample_rate = flac_reader.streaminfo().sample_rate as u32;
        //let channels = flac_reader.streaminfo().channels as u16;
        //let buffer_size = client.get_buffer_size()?;
        //let buffer_frame_count = buffer_size / wave_format.Format.nBlockAlign as u32;
        //let mut buffer = Vec::with_capacity(buffer_size as usize);
        //for frame in flac_reader.stream_blocks() {
        //    let pcm = frame.into_pcm().unwrap();
        //    for sample in pcm {
        //        for channel in sample {
        //            buffer.extend_from_slice(&channel.to_le_bytes());
        //        }
        //        if buffer.len() >= buffer_size as usize {
        //            client.write(buffer.as_slice(), buffer_frame_count, None)?;
        //            buffer.clear();
        //        }
        //    }
        //}
        //if !buffer.is_empty() {
        //    client.write(buffer.as_slice(), buffer_frame_count, None)?;
        //}
    //} 
    return Ok(());
}
