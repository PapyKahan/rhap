use clap::error::ErrorKind;
use clap::{CommandFactory, Parser};
use rand::seq::SliceRandom;
use rand::thread_rng;
use std::path::PathBuf;
use std::sync::Arc;
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
        let wasapi = match Host::new() {
            Ok(wasapi) => wasapi,
            Err(err) => {
                println!("Error initializing WASAPI: {:?}", err);
                return Err(());
            }
        };
        let devices = match wasapi.get_devices() {
            Ok(devices) => devices,
            Err(err) => {
                println!("Error enumerating devices: {:?}", err);
                return Err(());
            }
        };
        for dev in devices {
            println!("{} [{}]: {}", if dev.is_default { "->" } else { "  " }, dev.index, dev.get_name());
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


    let player = match Player::new(cli.device) {
        Ok(player) => Arc::new(player),
        Err(err) => {
            println!("Error initializing player: {:?}", err);
            return Err(());
        }
    };

    let player_clone = player.clone();
    match ctrlc::set_handler(move|| {
        println!("Stopping...");
        player_clone.stop().expect("Error stopping player");
        std::process::exit(0);
    }) {
        Ok(_) => {}
        Err(err) => {
            println!("Error setting Ctrl-C handler: {:?}", err);
            return Err(());
        }
    }

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
