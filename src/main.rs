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
use ratatui::Terminal;
use ui::App;
use std::io::stdout;
use std::path::PathBuf;

mod audio;
mod player;
mod ui;

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
            println!(
                "{} [{}]: {}",
                if dev.is_default() { "->" } else { "  " },
                index,
                dev.name()
            );
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

    //let host = audio::create_host("wasapi");
    //let mut player = Player::new(host, cli.device)?;
    //let cl = player.clone();
    tokio::spawn(async move {
        tokio::signal::ctrl_c()
            .await
            .expect("failed to listen for CTRL+C signal");
        //cl.stop();
        std::process::exit(0);
    });

    let mut out = stdout();
    execute!(out, EnterAlternateScreen, EnableMouseCapture)?;
    enable_raw_mode()?;

    let mut backend = ratatui::backend::CrosstermBackend::new(out);
    backend.execute(SetTitle("rhap - Rust Handcrafted Audio Player"))?;

    let mut terminal = Terminal::new(backend)?;
    let host = Host::new("wasapi");
    let mut app = App::new(host)?;
    app.run(&mut terminal)?;

    disable_raw_mode()?;
    let mut out = stdout();
    execute!(out, LeaveAlternateScreen, DisableMouseCapture)?;

    //let path = cli.path.expect("Error: A file or a path is expected");
    //if path.is_dir() {
    //    let mut files = WalkDir::new(path.clone())
    //        .follow_links(true)
    //        .into_iter()
    //        .filter_map(|e| e.ok())
    //        .filter(|e| {
    //            e.file_type().is_file()
    //                && e.file_name()
    //                    .to_str()
    //                    .map(|s| s.ends_with(".flac"))
    //                    .unwrap_or(false)
    //        })
    //        .map(|e| e.path().to_str().unwrap().to_string())
    //        .collect::<Vec<String>>();
    //    files.shuffle(&mut thread_rng());
    //    for f in files {
    //        player.play(f).await?;
    //    }
    //} else if path.is_file() {
    //    player
    //        .play(path.into_os_string().into_string().unwrap())
    //        .await?;
    //}
    Ok(())
}
