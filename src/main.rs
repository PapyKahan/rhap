use clap::error::ErrorKind;
use clap::{CommandFactory, Parser};
use rand::seq::SliceRandom;
use rand::thread_rng;
use std::path::PathBuf;
use walkdir::WalkDir;

mod audio;
mod player;

use crate::player::Player;
use crate::audio::{ HostTrait, DeviceTrait };

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
        let host = audio::create_host("wasapi");
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

    let host = audio::create_host("wasapi");
    let mut player = Player::new(host, cli.device)?;
    tokio::spawn(async move {
        tokio::signal::ctrl_c()
            .await
            .expect("failed to listen for CTRL+C signal");
        std::process::exit(0);
    });

    let path = cli.path.expect("Error: A file or a path is expected");
    if path.is_dir() {
        let mut files = WalkDir::new(path.clone())
            .follow_links(true)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| {
                e.file_type().is_file()
                    && e.file_name()
                        .to_str()
                        .map(|s| s.ends_with(".flac"))
                        .unwrap_or(false)
            })
            .map(|e| e.path().to_str().unwrap().to_string())
            .collect::<Vec<String>>();
        files.shuffle(&mut thread_rng());
        for f in files {
            player.play(f).await?;
        }
    } else if path.is_file() {
        player.play(path.into_os_string().into_string().unwrap()).await?;
    }
    return Ok(());
}
