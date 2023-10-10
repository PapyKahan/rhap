use clap::error::ErrorKind;
use clap::{CommandFactory, Parser};
use crossterm::event::{EnableMouseCapture, DisableMouseCapture, self, Event, KeyCode};
use crossterm::terminal::{enable_raw_mode, EnterAlternateScreen, disable_raw_mode, LeaveAlternateScreen, SetTitle};
use crossterm::{execute, ExecutableCommand};
use rand::seq::SliceRandom;
use rand::thread_rng;
use ratatui::widgets::{Paragraph, Block, Borders};
use ratatui::{Terminal, Frame};
use std::io::{stdout, self};
use std::path::PathBuf;
use walkdir::WalkDir;

mod audio;
mod player;

use crate::audio::{DeviceTrait, HostTrait};
use crate::player::Player;

#[derive(Parser)]
struct Cli {
    #[clap(short, long)]
    list: bool,
    #[clap(short, long)]
    path: Option<PathBuf>,
    #[clap(short, long)]
    device: Option<u32>,
}

fn ui(frame: &mut Frame<ratatui::backend::CrosstermBackend<std::io::Stdout>>) {
    frame.render_widget(
        Paragraph::new("Hello World!")
            .block(Block::default().title("Greeting").borders(Borders::ALL)),
        frame.size(),
    );
}

fn handle_events() -> io::Result<bool> {
    if event::poll(std::time::Duration::from_millis(50))? {
        if let Event::Key(key) = event::read()? {
            if key.kind == event::KeyEventKind::Press && key.code == KeyCode::Char('q') {
                return Ok(true);
            }
       }
    }
    Ok(false)
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

    //let host = audio::create_host("wasapi");
    //let mut player = Player::new(host, cli.device)?;
    //let cl = player.clone();
    //tokio::spawn(async move {
    //    tokio::signal::ctrl_c()
    //        .await
    //        .expect("failed to listen for CTRL+C signal");
    //    cl.stop();
    //    std::process::exit(0);
    //});

    let mut out = stdout();
    execute!(out, EnterAlternateScreen, EnableMouseCapture)?;
    enable_raw_mode()?;

    let mut backend = ratatui::backend::CrosstermBackend::new(out);
    backend.execute(SetTitle("rhap - Rust Handcrafted Audio Player"))?;

    let mut terminal = Terminal::new(backend)?;
    let mut should_quit = false;
    while !should_quit {
        terminal.draw(ui)?;
        should_quit = handle_events()?;
    }
    
    disable_raw_mode()?;
    //let mut out = stdout();
    //execute!(out, LeaveAlternateScreen, DisableMouseCapture)?;

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
