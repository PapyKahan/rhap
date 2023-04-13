use std::fs::File;
use std::io::BufReader;
use rodio::{Decoder, cpal::traits::HostTrait, cpal::traits::DeviceTrait};
use rodio::cpal;

fn enumerate_devices() {
    println!("Supported hosts:\n  {:?}", cpal::ALL_HOSTS);
    let available_hosts = cpal::available_hosts();
    println!("Available hosts:\n  {:?}", available_hosts);

    for host_id in available_hosts {
        println!("{}", host_id.name());
        let host = cpal::host_from_id(host_id).unwrap();

        let default_in = host.default_input_device().map(|e| e.name().unwrap());
        let default_out = host.default_output_device().map(|e| e.name().unwrap());
        println!("  Default Input Device:\n    {:?}", default_in);
        println!("  Default Output Device:\n    {:?}", default_out);

        let devices = host.devices().unwrap();
        println!("  Devices: ");
        for (device_index, device) in devices.enumerate() {
            println!("  {}. \"{}\"", device_index + 1, device.name().unwrap());

            // Input configs
            if let Ok(conf) = device.default_input_config() {
                println!("    Default input stream config:\n      {:?}", conf);
            }
            let input_configs = match device.supported_input_configs() {
                Ok(f) => f.collect(),
                Err(e) => {
                    println!("    Error getting supported input configs: {:?}", e);
                    Vec::new()
                }
            };
            if !input_configs.is_empty() {
                println!("    All supported input stream configs:");
                for (config_index, config) in input_configs.into_iter().enumerate() {
                    println!(
                        "      {}.{}. {:?}",
                        device_index + 1,
                        config_index + 1,
                        config
                    );
                }
            }

            // Output configs
            if let Ok(conf) = device.default_output_config() {
                println!("    Default output stream config:\n      {:?}", conf);
            }
            let output_configs = match device.supported_output_configs() {
                Ok(f) => f.collect(),
                Err(e) => {
                    println!("    Error getting supported output configs: {:?}", e);
                    Vec::new()
                }
            };
            if !output_configs.is_empty() {
                println!("    All supported output stream configs:");
                for (config_index, config) in output_configs.into_iter().enumerate() {
                    println!(
                        "      {}.{}. {:?}",
                        device_index + 1,
                        config_index + 1,
                        config
                    );
                }
            }
        }
    }
}

fn main() {
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

    let playback_device = match rodio::cpal::default_host().default_output_device() {
        Some(device) => device,
        None => {
            println!("No playback device found");
            return;
        }
    };
    println!("Using device: {}", playback_device.name().unwrap());

    // Configure output stream
    let config = rodio::cpal::SupportedStreamConfig::new(
        2, 
        rodio::cpal::SampleRate(44100), 
        rodio::cpal::SupportedBufferSize::Range {
            min: 44100, 
            max: 44100
        }, 
        rodio::cpal::SampleFormat::I16
    );

    // Get a output stream handle to the selected physical sound device
    let (_stream, stream_handle) = match rodio::OutputStream::try_from_device_config(&playback_device, config) {
        Ok((stream, stream_handle)) => (stream, stream_handle),
        Err(err) => {
            println!("Error opening output stream: {}", err);
            return;
        },
    };

    let file = match File::open(file_path) {
        Ok(file) => file,
        Err(err) => {
            println!("Error opening file: {}", err);
            return;
        },
    };

    // Load a sound from a file, using a path relative to Cargo.toml
    let buffer = BufReader::new(file);

    // Decode that sound file into a source
    let source = match Decoder::new(buffer) {
        Ok(decoder) => decoder,
        Err(e) => {
            println!("Error: {}", e);
            return;
        }
    };

    // Play the sound directly on the device
    let sink = match rodio::Sink::try_new(&stream_handle) {
        Ok(sink) => sink,
        Err(e) => {
            println!("Error: {}", e);
            return;
        }
    };
    sink.append(source);
    
    // The sound plays in a separate audio thread,
    // so we need to keep the main thread alive while it's playing.
    sink.sleep_until_end();
}
