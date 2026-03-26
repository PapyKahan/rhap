use std::sync::Arc;

use anyhow::Result;

use crate::musictrack::MusicTrack;
use crate::player::{CurrentTrackInfo, Player};
use crate::ui::screens::Playlist;

pub enum PlaybackEvent {
    TrackChanged,
    Stopped,
    None,
}

pub struct PlaybackController {
    pub player: Player,
    pub playing_track: Option<CurrentTrackInfo>,
    pub playing_track_index: usize,
    pub automatically_play_next: bool,
}

impl PlaybackController {
    pub fn new(player: Player) -> Self {
        Self {
            player,
            playing_track: None,
            playing_track_index: 0,
            automatically_play_next: true,
        }
    }

    pub fn toggle_play_pause(&mut self, playlist: &Playlist) -> Result<PlaybackEvent> {
        if self.player.is_playing() {
            self.player.pause()?;
            Ok(PlaybackEvent::None)
        } else if self.playing_track.is_some() {
            self.player.resume()?;
            Ok(PlaybackEvent::None)
        } else {
            self.play_selected(playlist)
        }
    }

    pub fn stop(&mut self) -> Result<PlaybackEvent> {
        self.player.stop()?;
        self.playing_track = None;
        Ok(PlaybackEvent::Stopped)
    }

    pub fn auto_advance(&mut self, playlist: &Playlist) -> Result<PlaybackEvent> {
        if let Some(track) = &self.playing_track {
            if !track.is_streaming() && self.automatically_play_next {
                let max_retries = playlist.songs_len().min(5);
                for _ in 0..max_retries {
                    match self.next(playlist) {
                        Ok(PlaybackEvent::TrackChanged) => return Ok(PlaybackEvent::TrackChanged),
                        Ok(_) => break,
                        Err(e) => log::warn!("Skipping unplayable track: {}", e),
                    }
                }
            }
        }
        Ok(PlaybackEvent::None)
    }

    pub fn next(&mut self, playlist: &Playlist) -> Result<PlaybackEvent> {
        let len = playlist.songs_len();
        if len == 0 {
            return Ok(PlaybackEvent::None);
        }
        self.playing_track_index = (self.playing_track_index + 1) % len;
        self.play(playlist)
    }

    pub fn previous(&mut self, playlist: &Playlist) -> Result<PlaybackEvent> {
        let len = playlist.songs_len();
        if len == 0 {
            return Ok(PlaybackEvent::None);
        }
        if let Some(current_track) = &self.playing_track {
            let elapsed_time = current_track.get_elapsed_time();
            // Note: elapsed_time tracks decoded position, which is ahead of
            // audible position by the ring buffer + device buffer duration.
            // Using 5s threshold to account for this buffering lag.
            if elapsed_time.seconds > 5 {
                return self.play(playlist);
            }
        }
        self.playing_track_index = if self.playing_track_index == 0 {
            len - 1
        } else {
            self.playing_track_index - 1
        };
        self.play(playlist)
    }

    pub fn play_selected(&mut self, playlist: &Playlist) -> Result<PlaybackEvent> {
        if let Some(index) = playlist.selected_index() {
            self.playing_track_index = index;
            return self.play(playlist);
        }
        Ok(PlaybackEvent::None)
    }

    fn play(&mut self, playlist: &Playlist) -> Result<PlaybackEvent> {
        if let Some(slot) = playlist.songs().get(self.playing_track_index) {
            let song = slot.load();

            // If not yet probed, probe and open in one shot to avoid double I/O.
            let preloaded_handle = if !song.probed {
                let (probed, handle) = MusicTrack::probe_and_open(song.path.clone())?;
                slot.store(Arc::new(probed));
                Some(handle)
            } else {
                None
            };
            let song = Arc::clone(&slot.load());

            match self.player.play_gapless(song.clone()) {
                Ok(Some(info)) => {
                    self.playing_track = Some(info);
                    return Ok(PlaybackEvent::TrackChanged);
                }
                Ok(None) => {}
                Err(e) => log::warn!("Gapless failed: {}", e),
            }

            self.player.stop()?;
            self.playing_track = None;
            let current_track_info = match preloaded_handle {
                Some(handle) => self.player.play_with_handle(song, handle)?,
                None => self.player.play(song)?,
            };
            self.playing_track = Some(current_track_info);
            return Ok(PlaybackEvent::TrackChanged);
        }
        Ok(PlaybackEvent::None)
    }

    pub fn is_playing(&self) -> bool {
        self.player.is_playing()
    }

    pub fn is_paused(&self) -> bool {
        self.player.is_paused()
    }
}
