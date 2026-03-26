use anyhow::Result;

use crate::action::{Action, Layer};
use crate::media_controls::MediaControlsBackend;
use crate::media_sync::MediaSync;
#[cfg(target_os = "windows")]
use crate::notifications::NotificationsBackend;
use crate::playback::{PlaybackController, PlaybackEvent};
use crate::player::Player;
use crate::ui::component::RenderContext;
use crate::ui::screens::Playlist;
use crate::ui::theme::Theme;
use crate::ui::widgets::{DeviceSelector, SearchWidget};

pub struct AppState {
    pub playback: PlaybackController,
    pub media_sync: MediaSync,
    pub layers: Vec<Layer>,
    pub status_message: Option<String>,
}

impl AppState {
    pub fn new(
        player: Player,
        media_controls: Option<MediaControlsBackend>,
        #[cfg(target_os = "windows")] notifications: Option<NotificationsBackend>,
        #[cfg(not(target_os = "windows"))] notifications: Option<std::convert::Infallible>,
    ) -> Self {
        Self {
            playback: PlaybackController::new(player),
            #[cfg(target_os = "windows")]
            media_sync: MediaSync::new(media_controls, notifications),
            #[cfg(not(target_os = "windows"))]
            media_sync: MediaSync::new(media_controls, notifications),
            layers: vec![],
            status_message: None,
        }
    }

    pub fn render_context<'a>(&'a self, theme: &'a Theme) -> RenderContext<'a> {
        RenderContext {
            playing_track: self.playback.playing_track.as_ref(),
            playing_track_index: self.playback.playing_track_index,
            is_playing: self.playback.is_playing(),
            is_paused: self.playback.is_paused(),
            status_message: self.status_message.as_deref(),
            theme,
        }
    }

    pub fn auto_advance(&mut self, playlist: &Playlist) -> Result<()> {
        match self.playback.auto_advance(playlist)? {
            PlaybackEvent::TrackChanged => {
                if let Some(track) = &self.playback.playing_track {
                    self.media_sync.on_track_changed(track);
                }
                self.status_message = None;
            }
            _ => {}
        }
        Ok(())
    }

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
                match self.playback.toggle_play_pause(playlist) {
                    Ok(PlaybackEvent::TrackChanged) => {
                        if let Some(track) = &self.playback.playing_track {
                            self.media_sync.on_track_changed(track);
                        }
                        self.status_message = None;
                    }
                    Err(e) => {
                        self.status_message = Some(format!("{}", e));
                        log::warn!("Cannot play track: {}", e);
                    }
                    _ => {}
                }
            }
            Action::Stop => {
                self.playback.stop()?;
                self.media_sync.clear();
            }
            Action::NextTrack => {
                let result = self.playback.next(playlist);
                self.handle_playback_result(result);
            }
            Action::PreviousTrack => {
                let result = self.playback.previous(playlist);
                self.handle_playback_result(result);
            }
            Action::PlaySelected => {
                let result = self.playback.play_selected(playlist);
                self.handle_playback_result(result);
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
                self.playback.stop()?;
                self.media_sync.clear();
            }
            Action::Batch(actions) => {
                for a in actions {
                    self.process_action(a, playlist, search_widget, output_selector)?;
                }
            }
        }
        Ok(())
    }

    fn handle_playback_result(&mut self, result: Result<PlaybackEvent>) {
        match result {
            Ok(PlaybackEvent::TrackChanged) => {
                if let Some(track) = &self.playback.playing_track {
                    self.media_sync.on_track_changed(track);
                }
                self.status_message = None;
            }
            Err(e) => {
                self.status_message = Some(format!("{}", e));
                log::warn!("Cannot play track: {}", e);
            }
            _ => {}
        }
    }
}
