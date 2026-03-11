use anyhow::Result;
use audio::Host;
use clap::Parser;
use player::Player;
use std::path::PathBuf;
use ui::App;

mod audio;
mod logging;
mod musictrack;
mod player;
mod tools;
mod ui;

// Import logging functionality
use logging::log_to_file_only;

use crate::audio::{DeviceTrait, HostTrait};

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    #[clap(short, long)]
    list: bool,
    #[clap(short = 'H', long, default_value_t = false)]
    high_priority_mode: bool,
    #[clap(short, long, required = true)]
    path: PathBuf,
    #[clap(short, long)]
    device: Option<u32>,
    #[clap(long, default_value_t = false)]
    pollmode: bool,
    #[clap(long, help = "Audio backend to use (alsa, wasapi)")]
    backend: Option<String>,
    #[clap(long, help = "Test ALSA device initialization without UI")]
    test_device: bool,
}

fn setup_logging() -> Result<()> {
    // Clear and initialize log file
    let log_file = "rhap_debug.log";
    std::fs::write(log_file, format!("=== Rhap Debug Log Started at {} ===\n",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs()))?;
    Ok(())
}

/// Convert a WSL `/mnt/<drive>/...` path to a Windows `<DRIVE>:\...` path.
fn wsl_path_to_windows(path: PathBuf) -> PathBuf {
    let path_str = path.to_string_lossy();
    if let Some(rest) = path_str.strip_prefix("/mnt/") {
        if let Some((drive, remainder)) = rest.split_once('/') {
            if drive.len() == 1 && drive.chars().next().is_some_and(|c| c.is_ascii_alphabetic()) {
                let win_path = format!("{}:\\{}", drive.to_uppercase(), remainder.replace('/', "\\"));
                return PathBuf::from(win_path);
            }
        }
    }
    path
}

#[tokio::main]
async fn main() -> Result<()> {
    // Setup logging first
    if let Err(e) = setup_logging() {
        eprintln!("Failed to setup logging: {}", e);
    }

    log_to_file_only("INFO", "=== Rhap Starting ===");

    let args = Args::parse();

    // Determine audio backend
    let host_name = match args.backend {
        Some(backend) => backend,
        None => {
            if cfg!(target_os = "linux") {
                "alsa".to_string()
            } else {
                "wasapi".to_string()
            }
        }
    };

    log_to_file_only("INFO", &format!("Using audio backend: {}", host_name));

    if args.list {
        let host = Host::new(&host_name, args.high_priority_mode);
        let devices = host.get_devices()?;
        let mut index = 0;
        for device in devices {
            let capabilities = device.get_capabilities()?;
            println!(
                "{} [{}]: {}",
                if device.is_default()? { "->" } else { "  " },
                index,
                device.name()?
            );
            if let Some(bitrate) = capabilities.bits_per_samples.last() {
                println!("    Max bits per sample: {}bits", *bitrate as usize);
            }
            if let Some(rate) = capabilities.sample_rates.last() {
                println!("    Max sample rate: {}Hz", *rate as usize);
            }
            index = index + 1;
        }
        return Ok(());
    }

    // Test device initialization if requested
    if args.test_device {
        log_to_file_only("INFO", "=== Testing Device Initialization ===");

        let host = Host::new(&host_name, args.high_priority_mode);

        // Get the device to test
        let device_id = args.device.unwrap_or(0);
        log_to_file_only("INFO", &format!("Testing device {}", device_id));

        let mut device = host.create_device(Some(device_id))?;
        let device_name = device.name()?;

        log_to_file_only("INFO", &format!("Device name: {}", device_name));

        // Get device capabilities
        let capabilities = device.get_capabilities()?;
        log_to_file_only("INFO", &format!("Device capabilities: {:?}", capabilities));

        // Test with a 24-bit 44.1kHz stream directly
        log_to_file_only("INFO", "Testing with 24-bit 44.1kHz stream");

        let stream_params = audio::StreamParams {
            channels: 2,
            samplerate: audio::SampleRate::Rate44100Hz,
            bits_per_sample: audio::BitsPerSample::Bits24,
            exclusive: false,
            pollmode: args.pollmode,
        };

        log_to_file_only("INFO", &format!("Stream params: {:?}", stream_params));

        // Test device start
        match device.start(&stream_params) {
            Ok(tx) => {
                log_to_file_only("INFO", "Device started successfully");
                // Send a tiny bit of silence to verify it works
                let frames = 100;
                let samples_per_frame = 2; 
                let bytes_per_sample = 3; 
                let test_data = vec![0u8; frames * samples_per_frame * bytes_per_sample];
                
                for byte in test_data {
                    if let Err(e) = tx.send(audio::StreamingData::Data(byte)).await {
                        log_to_file_only("ERROR", &format!("Failed to send audio byte: {}", e));
                        break;
                    }
                }
                let _ = tx.send(audio::StreamingData::EndOfStream).await;
                log_to_file_only("INFO", "Test complete");
            }
            Err(e) => {
                log_to_file_only("ERROR", &format!("Failed to start device: {}", e));
            }
        }

        log_to_file_only("INFO", "=== Device Test Complete ===");
        return Ok(());
    }

    tokio::spawn(async move {
        tokio::signal::ctrl_c()
            .await
            .expect("failed to listen for CTRL+C signal");
        std::process::exit(0);
    });

    log_to_file_only("INFO", &format!("Initializing Rhap with audio backend: {}", host_name));
    
    let path = if host_name == "wasapi" && cfg!(target_os = "linux") {
        wsl_path_to_windows(args.path)
    } else {
        args.path
    };
    
    log_to_file_only("INFO", &format!("Music path: {:?}", path));

    let mut terminal = ratatui::init();
    let host = Host::new(&host_name, args.high_priority_mode);

    log_to_file_only("INFO", "Creating player...");
    let player = Player::new(host.clone(), args.device, args.pollmode)
        .map_err(|e| {
            log_to_file_only("ERROR", &format!("Failed to create player: {}", e));
            e
        })?;
    log_to_file_only("INFO", "Player created successfully");

    log_to_file_only("INFO", "Creating app...");
    let mut app = App::new(host, player, path)
        .map_err(|e| {
            log_to_file_only("ERROR", &format!("Failed to create app: {}", e));
            e
        })?;
    log_to_file_only("INFO", "App created successfully");

    log_to_file_only("INFO", "Starting UI...");
    app.run(&mut terminal).await
        .map_err(|e| {
            log_to_file_only("ERROR", &format!("UI error: {}", e));
            e
        })?;
    log_to_file_only("INFO", "UI completed successfully");

    ratatui::restore();
    log_to_file_only("INFO", "=== Rhap Finished Successfully ===");

    Ok(())
}
