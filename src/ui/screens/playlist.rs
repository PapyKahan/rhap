use std::{path::PathBuf, sync::Arc};

use anyhow::Result;
use rand::{rng, seq::SliceRandom};
use ratatui::{
    prelude::{Alignment, Constraint, Rect},
    style::Style,
    widgets::{Block, BorderType, Borders, Cell, Clear, Row, Table, TableState, Scrollbar, ScrollbarState},
    Frame,
    layout::{Layout, Direction, Constraint as LConstraint}
};
use walkdir::WalkDir;

use crate::{
    musictrack::MusicTrack,
    player::{CurrentTrackInfo, Player},
    ui::{
        widgets::CurrentlyPlayingWidget, HIGHLIGHT_COLOR, ROW_ALTERNATE_COLOR,
        ROW_ALTERNATE_COLOR_COL, ROW_COLOR, ROW_COLOR_COL,
    },
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
            currently_playing_widget: CurrentlyPlayingWidget::new(None),
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
        self.currently_playing_widget.clear();
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
        let table_and_scrollbar = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([LConstraint::Min(0), LConstraint::Length(1)])
            .split(Rect {
                x: area.x,
                y: area.y,
                width: area.width,
                height: area.height - 7, // Reduced by 7 (6 for widget + 1 for placeholder)
            });
        
        let table_area = table_and_scrollbar[0];
        let scrollbar_area = table_and_scrollbar[1];

        // Calculate scrollbar state based on selection and total items
        let selected = self.state.selected().unwrap_or(0);
        let max_items = self.songs.len().saturating_sub(1);
        
        let mut scrollbar_state = ScrollbarState::default()
            .content_length(max_items)
            .position(selected);

        let widget_area = Rect {
            x: area.x,
            y: area.y + area.height - 7, // Position the widget below the table
            width: area.width,
            height: 6, // Height for the widget
        };

        let placeholder_area = Rect {
            x: area.x,
            y: area.y + area.height - 1, // Position the placeholder at the very bottom
            width: area.width,
            height: 1, // Height of 1 line for the placeholder
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
        let table = Table::new(
            items,
            &[
                Constraint::Length(1),
                Constraint::Percentage(20),
                Constraint::Percentage(60),
                Constraint::Percentage(10),
                Constraint::Percentage(10),
            ],
        )
        .row_highlight_style(Style::default().fg(HIGHLIGHT_COLOR))
        .block(
            Block::default()
                .title(format!("Playlist - {}", self.songs.len()))
                .title_alignment(Alignment::Left)
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(HIGHLIGHT_COLOR)),
        );

        // Render the table
        frame.render_widget(Clear, table_area);
        frame.render_stateful_widget(table, table_area, &mut self.state);
        
        // Render the scrollbar
        frame.render_stateful_widget(
            Scrollbar::default()
                .orientation(ratatui::widgets::ScrollbarOrientation::VerticalRight)
                .symbols(ratatui::symbols::scrollbar::VERTICAL)
                .track_symbol(Some("│"))
                .thumb_symbol("█")
                .track_style(Style::default())
                .thumb_style(Style::default().fg(HIGHLIGHT_COLOR)),
            scrollbar_area,
            &mut scrollbar_state
        );

        // Render the CurrentlyPlayingWidget
        self.currently_playing_widget.render(frame, widget_area);
        
        // Render a blank placeholder at the bottom
        frame.render_widget(Clear, placeholder_area);

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

    pub fn search(&self, query: &str) -> Option<usize> {
        if query.is_empty() {
            return None;
        }

        let query = query.to_lowercase();
        for (index, song) in self.songs.iter().enumerate() {
            let title = song.title.to_lowercase();
            let artist = song.artist.to_lowercase();

            if title.contains(&query) || artist.contains(&query) {
                return Some(index);
            }
        }

        None
    }

    pub fn search_next(&self, current_index: Option<usize>, query: &str) -> Option<usize> {
        if query.is_empty() {
            return None;
        }

        let query = query.to_lowercase();
        let start_index = current_index.map(|idx| idx + 1).unwrap_or(0);
        
        // First, search from current position to end
        for index in start_index..self.songs.len() {
            if let Some(song) = self.songs.get(index) {
                let title = song.title.to_lowercase();
                let artist = song.artist.to_lowercase();

                if title.contains(&query) || artist.contains(&query) {
                    return Some(index);
                }
            }
        }
        
        // If we didn't find anything and we started from a non-zero position,
        // cycle around to the beginning
        if start_index > 0 {
            for index in 0..start_index {
                if let Some(song) = self.songs.get(index) {
                    let title = song.title.to_lowercase();
                    let artist = song.artist.to_lowercase();

                    if title.contains(&query) || artist.contains(&query) {
                        return Some(index);
                    }
                }
            }
        }

        None
    }

    pub fn search_prev(&self, current_index: Option<usize>, query: &str) -> Option<usize> {
        if query.is_empty() {
            return None;
        }

        let query = query.to_lowercase();
        
        // Get the current position or use the length of songs as starting point
        // (to wrap around to the end when starting from the beginning)
        let start_index = current_index.unwrap_or(0);
        
        // First, search backward from current position to beginning
        for index in (0..start_index).rev() {
            if let Some(song) = self.songs.get(index) {
                let title = song.title.to_lowercase();
                let artist = song.artist.to_lowercase();

                if title.contains(&query) || artist.contains(&query) {
                    return Some(index);
                }
            }
        }
        
        // If we didn't find anything and we're not at the end,
        // cycle around to the end of the list
        for index in (start_index..self.songs.len()).rev() {
            if let Some(song) = self.songs.get(index) {
                let title = song.title.to_lowercase();
                let artist = song.artist.to_lowercase();

                if title.contains(&query) || artist.contains(&query) {
                    return Some(index);
                }
            }
        }

        None
    }

    pub fn select_index(&mut self, index: usize) {
        if index < self.songs.len() {
            self.state.select(Some(index));
        }
    }
    
    pub fn selected_index(&self) -> Option<usize> {
        self.state.selected()
    }
}
