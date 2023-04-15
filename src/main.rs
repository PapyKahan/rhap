use std::fs::File;
use std::io::BufReader;
use std::ffi::OsStr;
use std::os::windows::ffi::OsStrExt;
use windows::Win32::Foundation::*;
use windows::Win32::Media::Audio::*;
use windows::win32::windows_media_devices::{AUDIO_CLIENT_SHAREMODE_EXCLUSIVE, AUDCLNT_STREAMFLAGS_EVENTCALLBACK};
use windows::win32::direct3d11::D3D_DRIVER_TYPE_NULL;
use windows::win32::direct3d9::D3D_SDK_VERSION;
use windows::win32::mmdeviceapi::{eRender, CLSCTX_ALL};
use windows::win32::com::{CoInitializeEx, COINIT_APARTMENTTHREADED};
use windows::win32::winrt::RoInitialize;

fn enumerate_devices() {
}

fn main() -> windows::Result<()> {
    let args = std::env::args().collect::<Vec<String>>();

    let file_path = match args.len() {
        2 => &args[1],
        _ => {
            println!("Usage: rhap <file>");
            enumerate_devices();
            ""
        },
    };

    if file_path.is_empty() {
        return;
    }

    let file = match File::open(file_path) {
        Ok(file) => file,
        Err(err) => {
            println!("Error opening file: {}", err);
            return;
        },
    };

    // Load a sound from a file, using a path relative to Cargo.toml
    let buffer = BufReader::new(file);
    unsafe {
        // Initialise les sous-systèmes COM et WinRT.
        CoInitializeEx(std::ptr::null_mut(), COINIT_APARTMENTTHREADED)?;
        RoInitialize(0)?;

        // Ouvre le fichier FLAC.
        let filename = "C:/Chemin/vers/le/fichier.flac";
        let file_handle = windows::file_system::File::open(filename)?;

        // Crée un périphérique audio WASAPI exclusif.
        let device = windows::media::audio::AudioDevice::get_default_audio_endpoint(eRender, windows::media::AUDIO_DEVICE_ROLE::Multimedia)?;
        let client = device.activate_audio_client()?;
        let format = client.get_mix_format()?;
        let mut wave_format = format.clone().into();
        wave_format.wFormatTag = windows::win32::mmreg::WAVE_FORMAT_EXTENSIBLE as u16;
        wave_format.SubFormat = windows::win32::ksmedia::KSDATAFORMAT_SUBTYPE_PCM;
        wave_format.Format.wBitsPerSample = 16;
        wave_format.Format.nBlockAlign = wave_format.Format.nChannels * wave_format.Format.wBitsPerSample / 8;
        wave_format.Format.nAvgBytesPerSec = wave_format.Format.nSamplesPerSec * wave_format.Format.nBlockAlign;
        let event_handle = windows::synch::Event::create(None, true, false, None)?;
        client.initialize(AUDIO_CLIENT_SHAREMODE_EXCLUSIVE, AUDCLNT_STREAMFLAGS_EVENTCALLBACK, 0, 0, &wave_format.Format, std::ptr::null())?;
        client.set_event_handle(event_handle.handle)?;

        // Lit le fichier FLAC et écrit les échantillons dans le périphérique audio.
        let flac_reader = claxon::FlacReader::new(file_handle)?;
        let sample_rate = flac_reader.streaminfo().sample_rate as u32;
        let channels = flac_reader.streaminfo().channels as u16;
        let buffer_size = client.get_buffer_size()?;
        let buffer_frame_count = buffer_size / wave_format.Format.nBlockAlign as u32;
        let mut buffer = Vec::with_capacity(buffer_size as usize);
        for frame in flac_reader.stream_blocks() {
            let pcm = frame.into_pcm().unwrap();
            for sample in pcm {
                for channel in sample {
                    buffer.extend_from_slice(&channel.to_le_bytes());
                }
                if buffer.len() >= buffer_size as usize {
                    client.write(buffer.as_slice(), buffer_frame_count, None)?;
                    buffer.clear();
                }
            }
        }
        if !buffer.is_empty() {
            client.write(buffer.as_slice(), buffer_frame_count, None)?;
        }

        // Nettoie les ressources.
        drop(flac_reader);
        drop(file_handle);
        drop(client);
        drop(device);
        drop(event_handle);

        // Libère les sous-syst
