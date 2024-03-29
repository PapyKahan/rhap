use anyhow::Result;
use audio::Host;
use clap::error::ErrorKind;
use clap::{CommandFactory, Parser};
use crossterm::event::{
    DisableMouseCapture, EnableMouseCapture
};
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen, SetTitle,
};
use crossterm::{execute, ExecutableCommand};
use player::Player;
use ratatui::Terminal;
use ui::App;
use std::io::stdout;
use std::path::PathBuf;

mod audio;
mod player;
mod song;
mod ui;
mod tools;

use crate::audio::{DeviceTrait, HostTrait};

#[derive(Parser)]
struct Cli {
    #[clap(short, long)]
    list: bool,
    #[clap(short, long)]
    path: Option<PathBuf>,
    #[clap(short, long)]
    device: Option<u32>,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();
    if cli.list {
        let host = Host::new("wasapi");
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
    } else if cli.path.is_none() {
        let mut cmd = Cli::command();
        cmd.error(
            ErrorKind::MissingRequiredArgument,
            "File or directory must be specified",
        )
        .exit();
    }

    let host = Host::new("wasapi");
    let player = Player::new(host, cli.device)?;
    tokio::spawn(async move {
        tokio::signal::ctrl_c()
            .await
            .expect("failed to listen for CTRL+C signal");
        std::process::exit(0);
    });

    let mut out = stdout();
    execute!(out, EnterAlternateScreen, EnableMouseCapture)?;
    enable_raw_mode()?;

    let mut backend = ratatui::backend::CrosstermBackend::new(out);
    backend.execute(SetTitle("rhap - Rust Handcrafted Audio Player"))?;

    let mut terminal = Terminal::new(backend)?;
    let path = cli.path.expect("Error: A file or a path is expected");
    let mut app = App::new(host, player, path)?;
    app.run(&mut terminal).await?;

    disable_raw_mode()?;
    let mut out = stdout();
    execute!(out, LeaveAlternateScreen, DisableMouseCapture)?;

    Ok(())
}
