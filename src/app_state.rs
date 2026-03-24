use std::io::Write;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;

use crate::action::{Action, Layer};
use crate::media_controls::{MediaControlsBackend, MediaControlsTrait, PlaybackStatus, TrackMetadata};
use crate::musictrack::MusicTrack;
#[cfg(target_os = "windows")]
use crate::notifications::{NotificationsBackend, NotificationContent, NotificationsTrait};
use crate::player::{CurrentTrackInfo, Player};
use crate::ui::component::RenderContext;
use crate::ui::screens::Playlist;
use crate::ui::theme::Theme;
use crate::ui::widgets::{DeviceSelector, SearchWidget};

struct CoverArtFile {
    _path: tempfile::TempPath,
    url: String,
    #[cfg(target_os = "windows")]
    fs_path: String,
}

/// UI-agnostic application state. Shared between terminal and future web UI.
pub struct AppState {
    pub player: Player,
    pub playing_track: Option<CurrentTrackInfo>,
    pub playing_track_index: usize,
    pub automatically_play_next: bool,
    pub layers: Vec<Layer>,
    pub status_message: Option<String>,
    pub media_controls: Option<MediaControlsBackend>,
    #[cfg(target_os = "windows")]
    pub notifications: Option<NotificationsBackend>,
    cover_art_file: Option<CoverArtFile>,
    metadata_dirty: bool,
}

impl AppState {
    pub fn new(
        player: Player,
        media_controls: Option<MediaControlsBackend>,
        #[cfg(target_os = "windows")] notifications: Option<NotificationsBackend>,
        #[cfg(not(target_os = "windows"))] _notifications: Option<std::convert::Infallible>,
    ) -> Self {
        Self {
            player,
            playing_track: None,
            playing_track_index: 0,
            automatically_play_next: true,
            layers: vec![],
            status_message: None,
            media_controls,
            #[cfg(target_os = "windows")]
            notifications,
            cover_art_file: None,
            metadata_dirty: false,
        }
    }

    /// Build a state snapshot for rendering (terminal) or serialization (web).
    pub fn render_context<'a>(&'a self, theme: &'a Theme) -> RenderContext<'a> {
        RenderContext {
            playing_track: self.playing_track.as_ref(),
            playing_track_index: self.playing_track_index,
            is_playing: self.player.is_playing(),
            is_paused: self.player.is_paused(),
            status_message: self.status_message.as_deref(),
            theme,
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
                } else if let Err(e) = self.play_selected(playlist) {
                    self.status_message = Some(format!("{}", e));
                    log::warn!("Cannot play track: {}", e);
                }
            }
            Action::Stop => {
                self.playing_track = None;
                self.metadata_dirty = true;
                self.cover_art_file = None;
                self.player.stop()?;
            }
            Action::NextTrack => {
                if let Err(e) = self.next(playlist) {
                    self.status_message = Some(format!("{}", e));
                    log::warn!("Cannot play track: {}", e);
                }
            }
            Action::PreviousTrack => {
                if let Err(e) = self.previous(playlist) {
                    self.status_message = Some(format!("{}", e));
                    log::warn!("Cannot play track: {}", e);
                }
            }
            Action::PlaySelected => {
                if let Err(e) = self.play_selected(playlist) {
                    self.status_message = Some(format!("{}", e));
                    log::warn!("Cannot play track: {}", e);
                }
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
                self.metadata_dirty = true;
                self.cover_art_file = None;
            }
            Action::Batch(actions) => {
                for a in actions {
                    self.process_action(a, playlist, search_widget, output_selector)?;
                }
            }
        }
        self.sync_media_controls();
        Ok(())
    }

    pub fn sync_media_controls(&mut self) {
        if let Some(mc) = self.media_controls.as_mut() {
            mc.pump_messages();
        }
        if let Some(track) = &self.playing_track {
            if self.metadata_dirty {
                if let Some(mc) = self.media_controls.as_mut() {
                    let cover_url = self.cover_art_file.as_ref().map(|f| f.url.as_str());
                    let metadata_with_cover = TrackMetadata {
                        title: &track.title,
                        artist: &track.artist,
                        album: &track.album,
                        duration: Some(Duration::from_secs(track.total_duration.seconds)),
                        cover_url,
                    };
                    if let Err(e) = mc.set_metadata(&metadata_with_cover) {
                        log::warn!("set_metadata failed with cover_url {:?}: {}", cover_url, e);
                        let _ = mc.set_metadata(&TrackMetadata {
                            cover_url: None,
                            ..metadata_with_cover
                        });
                    }
                }
                #[cfg(target_os = "windows")]
                if let Some(notif) = &self.notifications {
                    let cover_path = self.cover_art_file.as_ref().map(|f| f.fs_path.as_str());
                    let content = NotificationContent {
                        title: &track.title,
                        artist: &track.artist,
                        album: &track.album,
                        cover_art_path: cover_path,
                    };
                    if let Err(e) = notif.show_track_change(&content) {
                        log::warn!("Toast notification failed: {}", e);
                    }
                }
                self.metadata_dirty = false;
            }
            if let Some(mc) = self.media_controls.as_mut() {
                let status = if self.player.is_playing() {
                    PlaybackStatus::Playing
                } else {
                    PlaybackStatus::Paused
                };
                let _ = mc.set_playback(status);
            }
        } else if let Some(mc) = self.media_controls.as_mut() {
            let _ = mc.set_playback(PlaybackStatus::Stopped);
        }
    }

    fn update_cover_art_file(&mut self) {
        self.cover_art_file = None;
        if let Some(track) = &self.playing_track {
            if let Some(data) = &track.cover_art {
                match (|| -> Result<CoverArtFile> {
                    let suffix = match track.cover_art_mime.as_deref() {
                        Some("image/png") => ".png",
                        _ => ".jpg",
                    };
                    let mut file = tempfile::Builder::new()
                        .prefix("rhap_cover_")
                        .suffix(suffix)
                        .tempfile_in(std::env::temp_dir())?;
                    file.write_all(data)?;
                    file.flush()?;
                    // Close the file handle so Windows Storage API can read it
                    let temp_path = file.into_temp_path();
                    // Canonicalize to resolve 8.3 short names (e.g. CHRIST~1.FAJ)
                    let full_path = std::fs::canonicalize(&temp_path)
                        .unwrap_or_else(|_| temp_path.to_path_buf());
                    // Windows Storage API needs backslash paths; strip \\?\ UNC prefix from canonicalize
                    let path_str = full_path.to_string_lossy();
                    let path_str = path_str.strip_prefix(r"\\?\").unwrap_or(&path_str);
                    let url = format!("file://{}", path_str);
                    Ok(CoverArtFile {
                        _path: temp_path,
                        url,
                        #[cfg(target_os = "windows")]
                        fs_path: path_str.to_string(),
                    })
                })() {
                    Ok(caf) => self.cover_art_file = Some(caf),
                    Err(e) => log::warn!("Failed to write cover art temp file: {}", e),
                }
            }
        }
    }

    fn play(&mut self, playlist: &Playlist) -> Result<()> {
        if let Some(slot) = playlist.songs().get(self.playing_track_index) {
            let song = slot.load();
            if !song.probed {
                let probed = MusicTrack::new(song.path.clone())?;
                slot.store(Arc::new(probed));
            }
            let song = Arc::clone(&slot.load());

            match self.player.play_gapless(song.clone()) {
                Ok(Some(info)) => {
                    self.playing_track = Some(info);
                    self.metadata_dirty = true;
                    self.update_cover_art_file();
                    self.status_message = None;
                    return Ok(());
                }
                Ok(None) => {}
                Err(e) => log::warn!("Gapless failed: {}", e),
            }

            self.player.stop()?;
            self.playing_track = None;
            self.metadata_dirty = true;
            let current_track_info = self.player.play(song)?;
            self.playing_track = Some(current_track_info);
            self.metadata_dirty = true;
            self.update_cover_art_file();
            self.status_message = None;
        }
        Ok(())
    }

    fn next(&mut self, playlist: &Playlist) -> Result<()> {
        let len = playlist.songs_len();
        if len == 0 {
            return Ok(());
        }
        self.playing_track_index = (self.playing_track_index + 1) % len;
        self.play(playlist)
    }

    fn previous(&mut self, playlist: &Playlist) -> Result<()> {
        let len = playlist.songs_len();
        if len == 0 {
            return Ok(());
        }
        if let Some(current_track) = &self.playing_track {
            let elapsed_time = current_track.get_elapsed_time();
            if elapsed_time.seconds > 3 {
                self.play(playlist)?;
                return Ok(());
            }
        }
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
