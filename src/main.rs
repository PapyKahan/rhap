use clap::error::ErrorKind;
use clap::{CommandFactory, Parser};
use crossterm::event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode};
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen, SetTitle,
};
use crossterm::{execute, ExecutableCommand};
use rand::seq::SliceRandom;
use rand::thread_rng;
use ratatui::prelude::{Backend, Constraint, Rect, Layout, Direction};
use ratatui::style::{Color, Modifier, Style};
use ratatui::widgets::{Block, Borders, Cell, List, ListItem, Paragraph, Row, Table, TableState};
use ratatui::{Frame, Terminal};
use std::io::{self, stdout};
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

struct DeviceList<'devicelist> {
    state: TableState,
    devices: Vec<Row<'devicelist>>,
}

impl<'devicelist> DeviceList<'devicelist> {
    fn new() -> DeviceList<'devicelist> {
        let host = audio::create_host("wasapi");
        let devices = match host.get_devices() {
            Ok(devices) => devices,
            Err(err) => {
                let mut cmd = Cli::command();
                cmd.error(ErrorKind::InvalidValue, err).exit();
            }
        };
        let mut index = 0;
        let mut items = Vec::new();

        for dev in devices {
            let row = Row::new(vec![
                Cell::from(if dev.is_default() { "*" } else { "  " }),
                Cell::from(index.to_string()),
                Cell::from(dev.name()),
            ])
            .height(1)
            .style(Style::default().fg(Color::White));
            items.push(row);
            index = index + 1;
        }
        DeviceList {
            state: TableState::default(),
            devices: items,
        }
    }

    fn next(&mut self) {
        let i = match self.state.selected() {
            Some(i) => {
                if i >= self.devices.len() - 1 {
                    0
                } else {
                    i + 1
                }
            }
            None => 0,
        };
        self.state.select(Some(i));
    }

    fn previous(&mut self) {
        let i = match self.state.selected() {
            Some(i) => {
                if i == 0 {
                    self.devices.len() - 1
                } else {
                    i - 1
                }
            }
            None => 0,
        };
        self.state.select(Some(i));
    }

    fn ui(&self) -> Table {
        let host = audio::create_host("wasapi");
        let devices = match host.get_devices() {
            Ok(devices) => devices,
            Err(err) => {
                let mut cmd = Cli::command();
                cmd.error(ErrorKind::InvalidValue, err).exit();
            }
        };
        let mut index = 0;
        let mut items = Vec::new();

        for dev in devices {
            let row = Row::new(vec![
                Cell::from(if dev.is_default() { "*" } else { "  " }),
                Cell::from(index.to_string()),
                Cell::from(dev.name()),
            ])
            .height(1)
            .style(Style::default().fg(Color::White));
            items.push(row);
            index = index + 1;
        }
        Table::new(items)
            .style(Style::default().fg(Color::White))
            .header(
                Row::new(vec![" ", "Id", "Device"])
                    .style(Style::default().fg(Color::White))
                    .height(1),
            )
            .highlight_symbol("=>")
            .highlight_style(Style::default().add_modifier(Modifier::BOLD))
            .widths(&[
                Constraint::Length(1),
                Constraint::Length(2),
                Constraint::Percentage(100),
            ])
            .column_spacing(1)
            .block(
                Block::default()
                    .title("Devices")
                    .borders(Borders::default()),
            )
    }
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

fn run_app<B: Backend>(terminal: &mut Terminal<B>, app: &mut DeviceList) -> io::Result<()> {
    loop {
        terminal.draw(|f| {
            f.render_stateful_widget(app.ui(), f.size(), &mut app.state.clone());
        })?;
        if event::poll(std::time::Duration::from_millis(50))? {
            if let Event::Key(key) = event::read()? {
                if key.kind == event::KeyEventKind::Press {
                    match key.code {
                        KeyCode::Char('q') => return Ok(()),
                        KeyCode::Down => app.next(),
                        KeyCode::Up => app.previous(),
                        _ => {}
                    }
                }
            }
        }
    }
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
    let mut d = DeviceList::new();
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
