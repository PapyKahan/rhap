use anyhow::Result;
use audio::Host;
use clap::error::ErrorKind;
use clap::{CommandFactory, Parser};
use crossterm::event::{
    DisableMouseCapture, EnableMouseCapture, KeyboardEnhancementFlags, PushKeyboardEnhancementFlags,
};
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, supports_keyboard_enhancement, EnterAlternateScreen, LeaveAlternateScreen, SetTitle
};
use crossterm::{execute, ExecutableCommand};
use player::Player;
use ratatui::Terminal;
use std::io::stdout;
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

    let mut out = stdout();
    if supports_keyboard_enhancement().unwrap() {
        execute!(
            out,
            EnterAlternateScreen,
            EnableMouseCapture,
            PushKeyboardEnhancementFlags(KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES)
        )?;
    }
    else {
        execute!(
            out,
            EnterAlternateScreen,
            EnableMouseCapture,
        )?;
    }

    enable_raw_mode()?;

    let mut backend = ratatui::backend::CrosstermBackend::new(out);
    backend.execute(SetTitle("rhap - Rust Handcrafted Audio Player"))?;

    let mut terminal = Terminal::new(backend)?;
    let path = args.path.expect("Error: A file or a path is expected");
    let mut app = App::new(host, player, path)?;
    app.run(&mut terminal).await?;

    disable_raw_mode()?;
    let mut out = stdout();
    execute!(out, LeaveAlternateScreen, DisableMouseCapture)?;

    Ok(())
}
