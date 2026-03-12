use anyhow::Result;
use audio::Host;
use clap::Parser;
use player::Player;
use std::path::PathBuf;
use ui::App;

mod audio;
mod musictrack;
mod player;
mod tools;
mod ui;

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

    let args = Args::parse();
    if args.list {
        let host = Host::new("wasapi", args.high_priority_mode);
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
                println!("    Max bits per sample: {}", bitrate);
            }
            if let Some(rate) = capabilities.sample_rates.last() {
                println!("    Max sample rate: {}", rate);
            }
            index = index + 1;
        }
        return Ok(());
    }

    tokio::spawn(async move {
        tokio::signal::ctrl_c()
            .await
            .expect("failed to listen for CTRL+C signal");
        std::process::exit(0);
    });

    let mut terminal = ratatui::init();
    let host = Host::new("wasapi", args.high_priority_mode);
    let player = Player::new(host, args.device, args.pollmode)?;
    let path = wsl_path_to_windows(args.path);
    let mut app = App::new(host, player, path)?;
    app.run(&mut terminal).await?;
    ratatui::restore();

    Ok(())
}
