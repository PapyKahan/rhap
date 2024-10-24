use anyhow::Result;
use audio::Host;
use clap::error::ErrorKind;
use clap::{CommandFactory, Parser};
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
    #[clap(short, long, default_value_t = false)]
    high_priority_mode: bool,
    #[clap(short, long)]
    path: Option<PathBuf>,
    #[clap(short, long)]
    device: Option<u32>,
    #[clap(long, default_value_t = false)]
    pollmode: bool,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();
    if args.list {
        let host = Host::new("wasapi", args.high_priority_mode);
        let devices = host.get_devices()?;
        let mut index = 0;
        for dev in devices {
            let capabilities = dev.get_capabilities()?;
            println!(
                "{} [{}]: {}",
                if dev.is_default()? { "->" } else { "  " },
                index,
                dev.name()?
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
    } else if args.path.is_none() {
        let mut cmd = Args::command();
        cmd.error(
            ErrorKind::MissingRequiredArgument,
            "File or directory must be specified",
        )
        .exit();
    }

    let host = Host::new("wasapi", args.high_priority_mode);
    let player = Player::new(host, args.device, args.pollmode)?;
    tokio::spawn(async move {
        tokio::signal::ctrl_c()
            .await
            .expect("failed to listen for CTRL+C signal");
        std::process::exit(0);
    });

    let mut terminal = ratatui::init();
    let path = args.path.expect("Error: A file or a path is expected");
    let mut app = App::new(host, player, path)?;
    app.run(&mut terminal).await?;

    ratatui::restore();

    Ok(())
}
