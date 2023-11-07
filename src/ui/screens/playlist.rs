use std::{path::PathBuf, sync::Arc};

use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent, KeyEventKind};
use rand::{seq::SliceRandom, thread_rng};
use ratatui::{
    prelude::{Alignment, Backend, Constraint, Rect},
    style::Style,
    widgets::{Block, BorderType, Borders, Cell, Clear, Row, Table, TableState},
    Frame,
};
use walkdir::WalkDir;

use crate::{
    player::{CurrentTrackInfo, Player},
    song::Song,
    ui::{HIGHLIGHT_COLOR, ROW_ALTERNATE_COLOR, ROW_ALTERNATE_COLOR_COL, ROW_COLOR, ROW_COLOR_COL},
};

pub struct Playlist {
    state: TableState,
    songs: Vec<Arc<Song>>,
    player: Player,
    playing_track: Option<CurrentTrackInfo>,
    playing_track_list_index: usize,
    automatically_play_next: bool,
}

impl Playlist {
    pub fn new(path: PathBuf, player: Player) -> Result<Self> {
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
            for f in files {
                songs.push(Arc::new(Song::new(f)?));
            }
        } else if path.is_file() {
            songs.push(Arc::new(Song::new(
                path.into_os_string().into_string().unwrap(),
            )?));
        }
        let mut state = TableState::default();
        state.select(Some(0));
        Ok(Self {
            state,
            songs,
            player,
            playing_track: None,
            playing_track_list_index: 0,
            automatically_play_next: true,
        })
    }

    pub fn select_next(&mut self) {
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

    pub fn select_previous(&mut self) {
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

    async fn next(&mut self) -> Result<()> {
        self.playing_track_list_index = if self.playing_track_list_index + 1 > self.songs.len() - 1
        {
            0
        } else {
            self.playing_track_list_index + 1
        };
        self.play().await
    }

    async fn previous(&mut self) -> Result<()> {
        self.playing_track_list_index = if self.playing_track_list_index == 0 {
            self.songs.len() - 1
        } else {
            self.playing_track_list_index - 1
        };
        self.play().await
    }

    async fn play(&mut self) -> Result<()> {
        self.stop().await?;
        if let Some(song) = self.songs.get(self.playing_track_list_index) {
            let current_track_info = self.player.play(song.clone()).await?;
            self.playing_track = Some(current_track_info);
        }
        Ok(())
    }

    async fn stop(&mut self) -> Result<()> {
        self.playing_track = None;
        self.player.stop()
    }

    pub async fn event_hanlder(&mut self, key: KeyEvent) -> Result<()> {
        if key.kind == KeyEventKind::Press {
            match key.code {
                KeyCode::Up | KeyCode::Char('k') => self.select_previous(),
                KeyCode::Down | KeyCode::Char('j') => self.select_next(),
                KeyCode::Enter => {
                    if let Some(index) = self.state.selected() {
                        self.playing_track_list_index = index;
                    } else {
                        self.playing_track_list_index = 0;
                    }
                    self.play().await?;
                }
                KeyCode::Char('s') => {
                    self.stop().await?;
                }
                KeyCode::Char('n') => {
                    self.next().await?;
                }
                KeyCode::Char('p') => {
                    self.previous().await?;
                }
                _ => (),
            }
        }
        Ok(())
    }

    pub async fn run(&mut self) -> Result<()> {
        if let Some(current_track) = self.playing_track.clone() {
            if !current_track.is_streaming() && self.automatically_play_next {
                self.next().await?;
            }
        }
        Ok(())
    }

    pub(crate) fn render<B: Backend>(&mut self, frame: &mut Frame<B>, area: Rect) -> Result<()> {
        let mut items = Vec::new();
        for index in 0..self.songs.len() {
            if let Some(song) = self.songs.get(index) {
                let row = Row::new(vec![
                    Cell::from(if self.playing_track_list_index == index {
                        "󰐊"
                    } else {
                        "  "
                    }),
                    Cell::from(song.title.clone()).style(Style::default().bg(
                        if items.len() % 2 == 0 {
                            ROW_COLOR_COL
                        } else {
                            ROW_ALTERNATE_COLOR_COL
                        },
                    )),
                    Cell::from(song.artist.clone()),
                    Cell::from(song.info()).style(Style::default().bg(if items.len() % 2 == 0 {
                        ROW_COLOR_COL
                    } else {
                        ROW_ALTERNATE_COLOR_COL
                    })),
                    Cell::from(song.formated_duration()),
                ])
                .height(1)
                .style(Style::default().bg(if items.len() % 2 == 0 {
                    ROW_COLOR
                } else {
                    ROW_ALTERNATE_COLOR
                }));
                items.push(row);
            }
        }
        let table = Table::new(items)
            .highlight_style(Style::default().fg(HIGHLIGHT_COLOR))
            .widths(&[
                Constraint::Length(1),
                Constraint::Percentage(20),
                Constraint::Percentage(60),
                Constraint::Percentage(10),
                Constraint::Percentage(10),
            ])
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
