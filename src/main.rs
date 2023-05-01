use clap::error::ErrorKind;
use clap::{CommandFactory, Parser};
use rand::seq::SliceRandom;
use rand::thread_rng;
use std::path::PathBuf;
use walkdir::WalkDir;

mod audio;
mod player;

use crate::audio::DeviceTrait;
use crate::player::Player;
use crate::audio::api::wasapi::host::Host;

#[derive(Parser)]
struct Cli {
    #[clap(short, long)]
    list: bool,
    #[clap(short, long)]
    path: Option<PathBuf>,
    #[clap(short, long)]
    device: Option<u32>,
}

fn main() -> Result<(), ()> {
    let cli = Cli::parse();
    if cli.list {
        let devices = Host::enumerate_devices().unwrap();
        for dev in devices {
            println!("Device: id={}, name={}", dev.index, dev.get_name());
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

    let player = Player::new(cli.device.unwrap_or_default());

    if cli.path.is_some() {
        let path = cli.path.clone().unwrap();
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
                player.play(f).expect("Error playing file");
            }

            println!("Directory: {}", path.to_str().unwrap());
        } else if path.is_file() {
            player.play(path.to_str().unwrap().to_string()).expect("Error playing file");
        }
    }
    return Ok(());
}
