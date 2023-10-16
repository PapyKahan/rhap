use anyhow::Result;
use audio::{Device, Host};
use clap::error::ErrorKind;
use clap::{CommandFactory, Parser};
use crossterm::event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode};
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen, SetTitle,
};
use crossterm::{execute, ExecutableCommand};
use rand::seq::SliceRandom;
use rand::thread_rng;
use ratatui::prelude::{Backend, Constraint, Direction, Layout, Rect};
use ratatui::widgets::{
    Block, Borders, Clear
};
use ratatui::{Frame, Terminal};
use ui::widgets::DeviceSelector;
use std::io::{self, stdout};
use std::path::PathBuf;
use walkdir::WalkDir;

mod audio;
mod player;
mod ui;

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


/// helper function to create a centered rect using up certain percentage of the available rect `r`
fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(r);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
}

fn ui<B: Backend>(frame: &mut Frame<B>, app: &mut DeviceSelector) -> Result<()> {
    let size = frame.size();

    let block = Block::default().title("Content").borders(Borders::ALL);
    frame.render_widget(block, size);

    if app.show_popup {
        let area = centered_rect(20, 10, size);
        frame.render_widget(Clear, area); //this clears out the background
        frame.render_stateful_widget(app.ui()?, area, &mut app.state.clone());
    }
    Ok(())
}

fn run_app<B: Backend>(terminal: &mut Terminal<B>, app: &mut DeviceSelector) -> Result<()> {
    loop {
        terminal.draw(|f| match ui(f, app) {
            Ok(ok) => ok,
            Err(err) => {
                println!("error while drawing {}", err.to_string());
                ()
            }
        })?;
        if event::poll(std::time::Duration::from_millis(50))? {
            if let Event::Key(key) = event::read()? {
                if key.kind == event::KeyEventKind::Press {
                    match key.code {
                        KeyCode::Char('q') => return Ok(()),
                        KeyCode::Enter => app.set_selected_device()?,
                        KeyCode::Char('p') => app.show_popup = !app.show_popup,
                        KeyCode::Down => app.next(),
                        KeyCode::Up => app.previous(),
                        _ => {}
                    }
                }
            }
        }
    }
}

pub struct App<'app> {
    screens: Vec<Block<'app>>
}

impl<'app> App<'app>{
    fn new() -> Self {
        Self{
            screens: vec!()
        }
    }
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
    let app = App::new();
    let host = Host::new("wasapi");
    //app.screens.push()
    let mut d = DeviceSelector::new(host)?;
    run_app(&mut terminal, &mut d)?;

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
