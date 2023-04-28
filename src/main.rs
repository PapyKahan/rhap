//
// TODO add commandline parsing : https://docs.rs/clap/latest/clap/
// reference : Shared mode streaming : https://learn.microsoft.com/en-us/windows/win32/coreaudio/rendering-a-stream
// reference : Exclusive mode streaming : https://learn.microsoft.com/en-us/windows/win32/coreaudio/exclusive-mode-streams
// reference : https://www.hresult.info/FACILITY_AUDCLNT
//
use clap::error::ErrorKind;
use clap::{CommandFactory, Parser};

mod audio;
mod player;
use crate::audio::api::wasapi::enumerate_devices;
use crate::player::Player;

#[derive(Parser)]
struct Cli {
    #[clap(short, long)]
    list: bool,
    #[clap(short, long)]
    file: Option<String>,
    #[clap(short, long)]
    device: Option<u16>,
}

fn main() -> Result<(), ()> {
    let cli = Cli::parse();
    if cli.list {
        let devices = enumerate_devices().unwrap();
        for dev in devices {
            println!("Device: id={}, name={}", dev.index, dev.name);
        }
        return Ok(());
    } else if cli.file.is_none() {
        let mut cmd = Cli::command();
            cmd.error(
                ErrorKind::MissingRequiredArgument,
                "Can't do relative and absolute version change",
            )
            .exit();
    }
    let file_path = cli.file.unwrap();

    let player = Player::new(cli.device.unwrap_or_default());
    player.play(file_path).expect("Error playing file");

    return Ok(());
}
