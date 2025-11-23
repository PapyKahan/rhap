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
        log_to_file_only("INFO", "=== Testing ALSA Device Initialization ===");

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

        // Test with a 24-bit 44.1kHz stream directly (simulating the user's issue)
        log_to_file_only("INFO", "Testing with 24-bit 44.1kHz stream (simulating user issue)");

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

                // First, let's see what format ALSA actually ended up with
                // We'll create a simple test to check the format
                log_to_file_only("INFO", "Testing ALSA format by creating appropriate audio data...");

                // Now we'll create 24-bit data to test the automatic conversion to 16-bit
                let frames = 1024;
                let samples_per_frame = 2; // stereo
                let bytes_per_sample = 3; // 24-bit
                let mut test_data = Vec::with_capacity(frames * samples_per_frame * bytes_per_sample);

                for frame in 0..frames {
                    // Generate a simple sine wave at 24-bit
                    let amplitude = 8388607.0; // 2^23 - 1 for 24-bit
                    let frequency = 440.0; // A4 note
                    let phase = (frame as f32 / 44100.0) * frequency * 2.0 * std::f32::consts::PI;
                    let sample_value = (amplitude * phase.sin()) as i32;

                    // Ensure it's in 24-bit range
                    let sample_value = sample_value.max(-8388608).min(8388607);

                    // Convert to 24-bit little-endian bytes
                    let left_sample = sample_value;
                    let right_sample = sample_value;

                    // Left channel (3 bytes)
                    test_data.push((left_sample & 0xFF) as u8);
                    test_data.push(((left_sample >> 8) & 0xFF) as u8);
                    test_data.push(((left_sample >> 16) & 0xFF) as u8);

                    // Right channel (3 bytes)
                    test_data.push((right_sample & 0xFF) as u8);
                    test_data.push(((right_sample >> 8) & 0xFF) as u8);
                    test_data.push(((right_sample >> 16) & 0xFF) as u8);
                }

                log_to_file_only("INFO", &format!("Created {} bytes of test 24-bit audio data", test_data.len()));

                // Send the test audio data byte by byte
                log_to_file_only("INFO", &format!("Sending {} bytes of test audio data...", test_data.len()));
                for byte in test_data {
                    if let Err(e) = tx.send(audio::StreamingData::Data(byte)).await {
                        log_to_file_only("ERROR", &format!("Failed to send audio byte: {}", e));
                        break;
                    }
                }
                log_to_file_only("INFO", "Test audio data sent successfully");

                // Send end of stream
                let _ = tx.send(audio::StreamingData::EndOfStream).await;
                log_to_file_only("INFO", "End of stream sent - test complete");
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
    log_to_file_only("INFO", &format!("Music path: {:?}", args.path));

    let mut terminal = ratatui::init();
    let host = Host::new(&host_name, args.high_priority_mode);

    // Test if we can access the path first
    if args.path.exists() {
        if args.path.is_dir() {
            log_to_file_only("INFO", &format!("Music directory: {}", args.path.display()));
            // List files in directory
            if let Ok(entries) = std::fs::read_dir(&args.path) {
                log_to_file_only("INFO", "Files found in directory:");
                for entry in entries.take(10) { // Limit to first 10 files
                    if let Ok(entry) = entry {
                        log_to_file_only("INFO", &format!("  - {}", entry.path().display()));
                    }
                }
            }
        } else {
            log_to_file_only("INFO", &format!("Music file: {}", args.path.display()));
            // Check file type
            if let Ok(metadata) = std::fs::metadata(&args.path) {
                log_to_file_only("INFO", &format!("File size: {} bytes", metadata.len()));
            }
            // Try to determine file type
            if let Ok(output) = std::process::Command::new("file")
                .arg(&args.path)
                .output()
            {
                if let Ok(file_info) = String::from_utf8(output.stdout) {
                    log_to_file_only("INFO", &format!("File type: {}", file_info.trim()));
                }
            }
        }
    } else {
        log_to_file_only("ERROR", &format!("Path does not exist: {}", args.path.display()));
        return Err(anyhow::anyhow!("Path does not exist: {}", args.path.display()));
    }

    log_to_file_only("INFO", "Creating player...");
    let player = Player::new(host.clone(), args.device, args.pollmode)
        .map_err(|e| {
            log_to_file_only("ERROR", &format!("Failed to create player: {}", e));
            e
        })?;
    log_to_file_only("INFO", "Player created successfully");

    log_to_file_only("INFO", "Creating app...");
    let mut app = App::new(host, player, args.path)
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

    log_to_file_only("INFO", "Restoring terminal...");
    ratatui::restore();
    log_to_file_only("INFO", "=== Rhap Finished Successfully ===");

    Ok(())
}
