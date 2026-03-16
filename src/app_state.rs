use std::sync::Arc;

use anyhow::Result;

use crate::action::{Action, Layer};
use crate::musictrack::MusicTrack;
use crate::player::{CurrentTrackInfo, Player};
use crate::ui::component::RenderContext;
use crate::ui::screens::Playlist;
use crate::ui::widgets::{DeviceSelector, SearchWidget};

/// UI-agnostic application state. Shared between terminal and future web UI.
pub struct AppState {
    pub player: Player,
    pub playing_track: Option<CurrentTrackInfo>,
    pub playing_track_index: usize,
    pub automatically_play_next: bool,
    pub layers: Vec<Layer>,
}

impl AppState {
    pub fn new(player: Player) -> Self {
        Self {
            player,
            playing_track: None,
            playing_track_index: 0,
            automatically_play_next: true,
            layers: vec![],
        }
    }

    /// Build a state snapshot for rendering (terminal) or serialization (web).
    pub fn render_context(&self) -> RenderContext<'_> {
        RenderContext {
            playing_track: self.playing_track.as_ref(),
            playing_track_index: self.playing_track_index,
            is_playing: self.player.is_playing(),
            is_paused: self.player.is_paused(),
        }
    }

    /// Check if auto-advance should trigger.
    pub fn auto_advance(&mut self, playlist: &Playlist) -> Result<()> {
        if let Some(track) = &self.playing_track {
            if !track.is_streaming() && self.automatically_play_next {
                let len = playlist.songs_len();
                for _ in 0..len {
                    match self.next(playlist) {
                        Ok(()) => break,
                        Err(e) => log::warn!("Skipping unplayable track: {}", e),
                    }
                }
            }
        }
        Ok(())
    }

    /// Process an Action. Single entry point for all state mutations.
    /// Both terminal App and future web server call this.
    pub fn process_action(
        &mut self,
        action: Action,
        playlist: &mut Playlist,
        search_widget: &mut SearchWidget,
        output_selector: &mut DeviceSelector,
    ) -> Result<()> {
        match action {
            Action::None => {}
            Action::TogglePlayPause => {
                if self.player.is_playing() {
                    self.player.pause()?;
                } else if self.playing_track.is_some() {
                    self.player.resume()?;
                } else {
                    self.play_selected(playlist)?;
                }
            }
            Action::Stop => {
                self.playing_track = None;
                self.player.stop()?;
            }
            Action::NextTrack => {
                self.next(playlist)?;
            }
            Action::PreviousTrack => {
                self.previous(playlist)?;
            }
            Action::PlaySelected => {
                self.play_selected(playlist)?;
            }
            Action::SelectUp => {
                playlist.select_previous();
            }
            Action::SelectDown => {
                playlist.select_next();
            }
            Action::PushLayer(layer) => {
                if matches!(layer, Layer::Search) {
                    search_widget.clear();
                }
                if matches!(layer, Layer::OutputSelector) {
                    output_selector.refresh_device_list()?;
                }
                self.layers.push(layer);
            }
            Action::PopLayer => {
                self.layers.pop();
            }
            Action::SearchQuery(query) => {
                let result = if !query.is_empty() {
                    playlist.search(&query)
                } else {
                    None
                };
                search_widget.set_search_result(result);
            }
            Action::SearchNext(query) => {
                if !query.is_empty() {
                    let current_index = playlist.selected_index();
                    if let Some(index) = playlist.search_next(current_index, &query) {
                        playlist.select_index(index);
                    }
                }
            }
            Action::SearchPrev(query) => {
                if !query.is_empty() {
                    let current_index = playlist.selected_index();
                    if let Some(index) = playlist.search_prev(current_index, &query) {
                        playlist.select_index(index);
                    }
                }
            }
            Action::CommitSearch(index) => {
                if let Some(idx) = index {
                    playlist.select_index(idx);
                }
            }
            Action::Quit => {
                self.player.stop()?;
                self.playing_track = None;
            }
            Action::Batch(actions) => {
                for a in actions {
                    self.process_action(a, playlist, search_widget, output_selector)?;
                }
            }
        }
        Ok(())
    }

    fn play(&mut self, playlist: &Playlist) -> Result<()> {
        self.player.stop()?;
        if let Some(slot) = playlist.songs().get(self.playing_track_index) {
            let song = slot.load();
            if !song.probed {
                let probed = MusicTrack::new(song.path.clone())?;
                slot.store(Arc::new(probed));
            }
            let song = Arc::clone(&slot.load());
            let current_track_info = self.player.play(song)?;
            self.playing_track = Some(current_track_info);
        }
        Ok(())
    }

    fn next(&mut self, playlist: &Playlist) -> Result<()> {
        let len = playlist.songs_len();
        self.playing_track_index = if self.playing_track_index + 1 > len - 1 {
            0
        } else {
            self.playing_track_index + 1
        };
        self.play(playlist)
    }

    fn previous(&mut self, playlist: &Playlist) -> Result<()> {
        if let Some(current_track) = &self.playing_track {
            let elapsed_time = current_track.get_elapsed_time();
            if elapsed_time.seconds > 3 {
                self.play(playlist)?;
                return Ok(());
            }
        }
        let len = playlist.songs_len();
        self.playing_track_index = if self.playing_track_index == 0 {
            len - 1
        } else {
            self.playing_track_index - 1
        };
        self.play(playlist)
    }

    fn play_selected(&mut self, playlist: &Playlist) -> Result<()> {
        if let Some(index) = playlist.selected_index() {
            self.playing_track_index = index;
            self.play(playlist)?;
        }
        Ok(())
    }
}
