use std::{path::PathBuf, sync::Arc, sync::atomic::{AtomicBool, Ordering}};

use anyhow::Result;
use arc_swap::ArcSwap;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use glob::glob;
use rand::{rng, seq::SliceRandom};
use ratatui::{
    layout::{Constraint as LConstraint, Direction, Layout},
    prelude::{Alignment, Constraint, Rect},
    widgets::{
        Block, BorderType, Borders, Cell, Clear, Row, Scrollbar, ScrollbarState, Table, TableState,
    },
    Frame,
};
use rayon::prelude::*;

use crate::{
    action::{Action, Layer},
    musictrack::MusicTrack,
    player::format_time,
    ui::{
        component::{Component, RenderContext},
        widgets::CurrentlyPlayingWidget,
    },
};

pub struct Playlist {
    state: TableState,
    songs: Arc<[ArcSwap<MusicTrack>]>,
    currently_playing_widget: CurrentlyPlayingWidget,
    prober_cancel: Arc<AtomicBool>,
}

impl Playlist {
    pub fn new(path: PathBuf, picker: Option<ratatui_image::picker::Picker>) -> Result<Self> {
        let songs: Arc<[ArcSwap<MusicTrack>]>;

        if path.is_dir() {
            const AUDIO_EXTENSIONS: &[&str] = &[
                "flac", "mp3", "wav", "ogg", "m4a", "aac", "opus", "mp4", "mka", "webm", "caf",
            ];
            let mut files: Vec<String> = Vec::new();
            for ext in AUDIO_EXTENSIONS {
                let pattern = format!("{}/**/*.{}", path.display(), ext);
                files.extend(
                    glob(&pattern)?
                        .filter_map(|entry| entry.ok())
                        .map(|path| path.to_string_lossy().to_string()),
                );
            }

            // Create unprobed entries instantly, then shuffle
            let mut entries: Vec<ArcSwap<MusicTrack>> = files
                .into_iter()
                .map(|f| ArcSwap::from_pointee(MusicTrack::from_path(f)))
                .collect();
            entries.shuffle(&mut rng());
            songs = Arc::from(entries);

            // Spawn background prober — stores directly into shared ArcSwap slots
            let songs_ref = songs.clone();
            let cancel = Arc::new(AtomicBool::new(false));
            let cancel_clone = Arc::clone(&cancel);

            let _ = std::thread::Builder::new()
                .name("rhap-prober".into())
                .spawn(move || {
                    songs_ref.par_iter().for_each(|slot| {
                        if cancel_clone.load(Ordering::Relaxed) {
                            return;
                        }
                        let path = slot.load().path.clone();
                        if let Ok(track) = MusicTrack::new(path) {
                            slot.store(Arc::new(track));
                        }
                    });
                });
            let mut state = TableState::default();
            state.select(Some(0));
            return Ok(Self {
                state,
                songs,
                currently_playing_widget: CurrentlyPlayingWidget::new(picker),
                prober_cancel: cancel,
            });
        }

        if path.is_file() {
            let path_str = path.into_os_string().into_string().unwrap();
            songs = Arc::from(vec![ArcSwap::from_pointee(MusicTrack::new(path_str)?)]);
        } else {
            songs = Arc::from(vec![]);
        }

        let mut state = TableState::default();
        state.select(Some(0));
        Ok(Self {
            state,
            songs,
            currently_playing_widget: CurrentlyPlayingWidget::new(picker),
            prober_cancel: Arc::new(AtomicBool::new(false)),
        })
    }

    pub fn songs(&self) -> &Arc<[ArcSwap<MusicTrack>]> {
        &self.songs
    }

    pub fn songs_len(&self) -> usize {
        self.songs.len()
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

    pub fn search(&self, query: &str) -> Option<usize> {
        if query.is_empty() {
            return None;
        }

        let query = query.to_lowercase();
        for (index, slot) in self.songs.iter().enumerate() {
            let song = slot.load();
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

        for index in start_index..self.songs.len() {
            if let Some(slot) = self.songs.get(index) {
                let song = slot.load();
                let title = song.title.to_lowercase();
                let artist = song.artist.to_lowercase();

                if title.contains(&query) || artist.contains(&query) {
                    return Some(index);
                }
            }
        }

        if start_index > 0 {
            for index in 0..start_index {
                if let Some(slot) = self.songs.get(index) {
                    let song = slot.load();
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
        let start_index = current_index.unwrap_or(0);

        for index in (0..start_index).rev() {
            if let Some(slot) = self.songs.get(index) {
                let song = slot.load();
                let title = song.title.to_lowercase();
                let artist = song.artist.to_lowercase();

                if title.contains(&query) || artist.contains(&query) {
                    return Some(index);
                }
            }
        }

        for index in (start_index..self.songs.len()).rev() {
            if let Some(slot) = self.songs.get(index) {
                let song = slot.load();
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

impl Drop for Playlist {
    fn drop(&mut self) {
        self.prober_cancel.store(true, Ordering::Relaxed);
        // Do not join the prober: rayon's par_iter cannot be cancelled
        // mid-task, so in-flight probes would block shutdown on a slow disk.
        // The process is exiting, so the OS will reap the thread.
    }
}

impl Component for Playlist {
    fn render(&mut self, frame: &mut Frame, area: Rect, ctx: &RenderContext) -> Result<()> {
        let widget_height: u16 = 9;

        let table_and_scrollbar = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([LConstraint::Min(0), LConstraint::Length(1)])
            .split(Rect {
                x: area.x,
                y: area.y,
                width: area.width,
                height: area.height - (widget_height + 1),
            });

        let table_area = table_and_scrollbar[0];
        let scrollbar_area = table_and_scrollbar[1];

        let selected = self.state.selected().unwrap_or(0);
        let max_items = self.songs.len().saturating_sub(1);

        let mut scrollbar_state = ScrollbarState::default()
            .content_length(max_items)
            .position(selected);

        let widget_area = Rect {
            x: area.x,
            y: area.y + area.height - (widget_height + 1),
            width: area.width,
            height: widget_height,
        };

        let placeholder_area = Rect {
            x: area.x,
            y: area.y + area.height - 1,
            width: area.width,
            height: 1,
        };

        let mut items = Vec::new();
        for index in 0..self.songs.len() {
            if let Some(slot) = self.songs.get(index) {
                let song = slot.load();
                let row = Row::new(vec![
                    Cell::from(if index == ctx.playing_track_index {
                        if ctx.is_paused {
                            "󰏤"
                        } else if ctx.is_playing {
                            "󰐊"
                        } else {
                            "  "
                        }
                    } else {
                        "  "
                    }),
                    Cell::from(song.title.clone()).style(if items.len() % 2 == 0 {
                        ctx.theme.table.cell_even
                    } else {
                        ctx.theme.table.cell_odd
                    }),
                    Cell::from(song.artist.clone()),
                    Cell::from(if song.probed {
                        song.info()
                    } else {
                        String::new()
                    })
                    .style(if items.len() % 2 == 0 {
                        ctx.theme.table.cell_even
                    } else {
                        ctx.theme.table.cell_odd
                    }),
                    Cell::from(if song.probed {
                        format_time(song.duration)
                    } else {
                        String::new()
                    }),
                ])
                .height(1)
                .style(if items.len() % 2 == 0 {
                    ctx.theme.table.row_even
                } else {
                    ctx.theme.table.row_odd
                });
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
        .row_highlight_style(ctx.theme.table.highlight)
        .block(
            Block::default()
                .title(format!("Playlist - {}", self.songs.len()))
                .title_alignment(Alignment::Left)
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(ctx.theme.border),
        );

        frame.render_widget(Clear, table_area);
        frame.render_stateful_widget(table, table_area, &mut self.state);

        frame.render_stateful_widget(
            Scrollbar::default()
                .orientation(ratatui::widgets::ScrollbarOrientation::VerticalRight)
                .symbols(ratatui::symbols::scrollbar::VERTICAL)
                .track_symbol(Some("│"))
                .begin_symbol(Some("│"))
                .thumb_symbol("│")
                .end_symbol(Some("│"))
                .track_style(ctx.theme.scrollbar.track)
                .thumb_style(ctx.theme.scrollbar.thumb)
                .begin_style(ctx.theme.scrollbar.track)
                .end_style(ctx.theme.scrollbar.track),
            scrollbar_area,
            &mut scrollbar_state,
        );

        self.currently_playing_widget.render(frame, widget_area, ctx);

        frame.render_widget(Clear, placeholder_area);

        Ok(())
    }

    fn handle_key_event(&mut self, key: KeyEvent) -> Result<Action> {
        match key.code {
            KeyCode::Char('q') => Ok(Action::Quit),
            KeyCode::Char('/') => Ok(Action::PushLayer(Layer::Search)),
            KeyCode::Char('o') => Ok(Action::PushLayer(Layer::OutputSelector)),
            KeyCode::Char('p') if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                Ok(Action::TogglePlayPause)
            }
            KeyCode::Char(' ') => Ok(Action::TogglePlayPause),
            KeyCode::Char('s') => Ok(Action::Stop),
            KeyCode::Char('l') => Ok(Action::NextTrack),
            KeyCode::Char('h') => Ok(Action::PreviousTrack),
            KeyCode::Up | KeyCode::Char('k') => Ok(Action::SelectUp),
            KeyCode::Down | KeyCode::Char('j') => Ok(Action::SelectDown),
            KeyCode::Enter => Ok(Action::PlaySelected),
            _ => Ok(Action::None),
        }
    }
}
