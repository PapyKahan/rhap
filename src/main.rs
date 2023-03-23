use std::fs::File;
use std::io::BufReader;
use rodio::{Decoder, cpal::traits::HostTrait, cpal::traits::DeviceTrait};

fn main() {
    let args = std::env::args().collect::<Vec<String>>();

    let mut file_path = &String::new();
    let mut playback_device = rodio::cpal::default_host().default_output_device().unwrap();

    match args.len() {
        2 => {
            file_path = &args[1];
        },
        _ => {
            println!("Usage: rhap <file>");
            let hosts = rodio::cpal::available_hosts();
            for host in hosts {
                let h = rodio::cpal::host_from_id(host).unwrap();
                let devices = h.output_devices().unwrap();
                for device in devices {
                    let configs = device.supported_output_configs().unwrap();
                    if device.name().unwrap().contains("Qudelix-5K USB DAC") {
                        println!("Host: {:?}", host);
                        println!("Device: {:?}", device.name().unwrap());
                        for config in configs {
                            println!("Config: {:?}", config);
                        }
                    }
                }
            }
            return;
        },
    }
    let hosts = rodio::cpal::available_hosts();
    for host in hosts {
        let h = rodio::cpal::host_from_id(host).unwrap();
        let devices = h.output_devices().unwrap();
        for device in devices {
            if device.name().unwrap().contains("Qudelix-5K USB DAC") {
                playback_device = device;
            }
        }
    }

    // Configure output stream
    let config = rodio::cpal::SupportedStreamConfig::new(2, rodio::cpal::SampleRate(44100), rodio::cpal::SupportedBufferSize::Range{ min: 192000, max: 192000 }, rodio::cpal::SampleFormat::I32);

    // Get a output stream handle to the selected physical sound device
    let (_stream, stream_handle) = rodio::OutputStream::try_from_device_config(&playback_device, config).unwrap();

    // Load a sound from a file, using a path relative to Cargo.toml
    let buffer = BufReader::new(File::open(file_path).unwrap());

    // Decode that sound file into a source
    let source = Decoder::new(buffer).unwrap();

    // Play the sound directly on the device
    let sink = rodio::Sink::try_new(&stream_handle).unwrap();
    sink.append(source);
    
    // The sound plays in a separate audio thread,
    // so we need to keep the main thread alive while it's playing.
    sink.sleep_until_end();
}
