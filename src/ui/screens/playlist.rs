use std::{path::PathBuf, sync::Arc};

use anyhow::Result;
use rand::{seq::SliceRandom, rng};
use ratatui::{
    prelude::{Alignment, Constraint, Rect, Span, Line},
    style::{Style, Modifier},
    widgets::{Block, BorderType, Borders, Cell, Clear, Paragraph, Row, Table, TableState},
    Frame,
};
use walkdir::WalkDir;

use crate::{
    player::{CurrentTrackInfo, Player},
    musictrack::MusicTrack,
    ui::{HIGHLIGHT_COLOR, ROW_ALTERNATE_COLOR, ROW_ALTERNATE_COLOR_COL, ROW_COLOR, ROW_COLOR_COL},
};

pub struct Playlist {
    state: TableState,
    songs: Vec<Arc<MusicTrack>>,
    player: Player,
    playing_track: Option<CurrentTrackInfo>,
    playing_track_index: usize,
    automatically_play_next: bool,
    currently_playing_widget: CurrentlyPlayingWidget,
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
            files.shuffle(&mut rng());
            for f in files {
                songs.push(Arc::new(MusicTrack::new(f)?));
            }
        } else if path.is_file() {
            songs.push(Arc::new(MusicTrack::new(
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
            playing_track_index: 0,
            automatically_play_next: true,
            currently_playing_widget: CurrentlyPlayingWidget::new(None)
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

    pub async fn play(&mut self) -> Result<()> {
        self.player.stop().await?;
        if let Some(song) = self.songs.get(self.playing_track_index) {
            let current_track_info = self.player.play(song.clone()).await?;
            self.playing_track = Some(current_track_info.clone());
            self.currently_playing_widget = CurrentlyPlayingWidget::new(Some(current_track_info));
        }
        Ok(())
    }

    pub async fn next(&mut self) -> Result<()> {
        self.playing_track_index = if self.playing_track_index + 1 > self.songs.len() - 1 {
            0
        } else {
            self.playing_track_index + 1
        };
        self.play().await
    }

    pub async fn previous(&mut self) -> Result<()> {
        self.playing_track_index = if self.playing_track_index == 0 {
            self.songs.len() - 1
        } else {
            self.playing_track_index - 1
        };
        self.play().await
    }

    pub async fn stop(&mut self) -> Result<()> {
        self.playing_track = None;
        self.player.stop().await
    }

    pub async fn pause(&mut self) -> Result<()> {
        self.player.pause()?;
        Ok(())
    }

    pub async fn resume(&mut self) -> Result<()> {
        if self.playing_track.is_some() {
            self.player.resume()?;
        } else {
            self.play_selected().await?;
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

    pub(crate) fn render(&mut self, frame: &mut Frame, area: Rect) -> Result<()> {
        let table_area = Rect {
            x: area.x,
            y: area.y,
            width: area.width,
            height: area.height - 5, // Adjust the height to leave space for the widget
        };

        let widget_area = Rect {
            x: area.x,
            y: area.y + area.height - 5, // Position the widget below the table
            width: area.width,
            height: 5, // Height for the widget
        };

        let mut items = Vec::new();
        for index in 0..self.songs.len() {
            if let Some(song) = self.songs.get(index) {
                let row = Row::new(vec![
                    Cell::from(if index == self.playing_track_index {
                        if self.player.is_paused() {
                            "󰏤"
                        } else if self.player.is_playing() {
                            "󰐊"
                        } else {
                            "  "
                        }
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
        let table = Table::new(items, &[
                Constraint::Length(1),
                Constraint::Percentage(20),
                Constraint::Percentage(60),
                Constraint::Percentage(10),
                Constraint::Percentage(10),
            ])
            .row_highlight_style(Style::default().fg(HIGHLIGHT_COLOR))
            .block(
                Block::default()
                    .title(format!("Playlist - {}", self.songs.len()))
                    .title_alignment(Alignment::Left)
                    .borders(Borders::ALL)
                    .border_type(BorderType::Rounded)
                    .border_style(Style::default().fg(HIGHLIGHT_COLOR)),
            );

        frame.render_widget(Clear, table_area);
        frame.render_stateful_widget(table, table_area, &mut self.state);

        // Render the CurrentlyPlayingWidget
        self.currently_playing_widget.render(frame, widget_area);

        Ok(())
    }

    pub async fn play_selected(&mut self) -> Result<()> {
        if let Some(index) = self.state.selected() {
            self.playing_track_index = index;
            self.play().await?;
        }
        Ok(())
    }

    pub fn is_playing(&self) -> bool {
        self.player.is_playing()
    }
}

pub struct CurrentlyPlayingWidget {
    track_info: Option<CurrentTrackInfo>,
}

impl CurrentlyPlayingWidget {
    pub fn new(track_info: Option<CurrentTrackInfo>) -> Self {
        Self { track_info }
    }

    pub fn render(&self, frame: &mut Frame, area: Rect) {
        let text = if let Some(track_info) = &self.track_info {
            vec![
                Line::from(vec![
                    Span::styled("Title: ", Style::default().add_modifier(Modifier::BOLD)),
                    Span::raw(&track_info.title),
                ]),
                Line::from(vec![
                    Span::styled("Artist: ", Style::default().add_modifier(Modifier::BOLD)),
                    Span::raw(&track_info.artist),
                ]),
                Line::from(vec![
                    Span::styled("Info: ", Style::default().add_modifier(Modifier::BOLD)),
                    Span::raw(format!("{}", track_info.info)),
                ]),
            ]
        } else {
            vec![Line::from(Span::raw("No track playing"))]
        };

        let paragraph = Paragraph::new(text)
            .block(
                Block::default()
                    .title("Currently Playing")
                    .title_alignment(Alignment::Center)
                    .borders(Borders::ALL)
                    .border_type(BorderType::Rounded)
                    .border_style(Style::default().fg(HIGHLIGHT_COLOR)),
            )
            .alignment(Alignment::Center);

        frame.render_widget(paragraph, area);
    }
}
