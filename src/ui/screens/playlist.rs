use std::path::PathBuf;

use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent, KeyEventKind};
use rand::{thread_rng, seq::SliceRandom};
use ratatui::{
    prelude::{Alignment, Backend, Constraint, Rect},
    style::Style,
    widgets::{Block, BorderType, Borders, Cell, Clear, Row, Table, TableState},
    Frame,
};
use walkdir::WalkDir;

use crate::ui::{HIGHLIGHT_COLOR, ROW_ALTERNATE_COLOR, ROW_COLOR};

pub struct Playlist {
    state: TableState,
    songs: Vec<String>,
}

impl Playlist {
    pub fn new(path: PathBuf) -> Self {
        let mut songs = vec![];
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
            songs = files;
            //for f in files {
            //    player.play(f).await?;
            //}
        } else if path.is_file() {
            songs.push(path.into_os_string().into_string().unwrap());
        //    player
        //        .play(path.into_os_string().into_string().unwrap())
        //        .await?;
        }
        Self {
            state: TableState::default(),
            songs
        }
    }

    pub fn next(&mut self) {
        let i = match self.state.selected() {
            Some(i) => {
                if i >= self.songs.len() - 1 {
                    0
                } else {
                    i + 1
                }
            }
            None => 0,
        };
        self.state.select(Some(i));
    }

    pub fn previous(&mut self) {
        let i = match self.state.selected() {
            Some(i) => {
                if i == 0 {
                    self.songs.len() - 1
                } else {
                    i - 1
                }
            }
            None => 0,
        };
        self.state.select(Some(i));
    }

    pub fn event_hanlder(&mut self, key: KeyEvent) -> Result<()> {
        if key.kind == KeyEventKind::Press {
            match key.code {
                KeyCode::Up => self.previous(),
                KeyCode::Down => self.next(),
                _ => (),
            }
        }
        Ok(())
    }

    pub(crate) fn render<B: Backend>(&mut self, frame: &mut Frame<B>, area: Rect) -> Result<()> {
        let mut items = Vec::new();
        for song in &self.songs {
            let row = Row::new(vec![
                //Cell::from(if is_selected { "󰓃" } else { "  " }),
                Cell::from(" "),
                Cell::from(song.to_string()),
            ])
            .height(1)
            .style(Style::default().bg(if items.len() % 2 == 0 {
                ROW_COLOR
            } else {
                ROW_ALTERNATE_COLOR
            }));
            items.push(row);
        }
        let table = Table::new(items)
            .highlight_style(Style::default().fg(HIGHLIGHT_COLOR))
            .widths(&[Constraint::Length(1), Constraint::Percentage(100)])
            .block(
                Block::default()
                    .title(format!("Playlist - {}", self.songs.len()))
                    .title_alignment(Alignment::Left)
                    .borders(Borders::ALL)
                    .border_type(BorderType::Rounded)
                    .border_style(Style::default().fg(HIGHLIGHT_COLOR)),
            );

        frame.render_widget(Clear, area);
        frame.render_stateful_widget(table, area, &mut self.state);
        Ok(())
    }
}
