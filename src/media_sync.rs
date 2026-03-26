use std::io::Write;
use std::time::Duration;

use anyhow::Result;

use crate::media_controls::{MediaControlsBackend, MediaControlsTrait, PlaybackStatus, TrackMetadata};
#[cfg(target_os = "windows")]
use crate::notifications::{NotificationsBackend, NotificationContent, NotificationsTrait};
use crate::playback::PlaybackController;
use crate::player::CurrentTrackInfo;

struct CoverArtFile {
    _path: tempfile::TempPath,
    url: String,
    #[cfg(target_os = "windows")]
    fs_path: String,
}

pub struct MediaSync {
    media_controls: Option<MediaControlsBackend>,
    #[cfg(target_os = "windows")]
    notifications: Option<NotificationsBackend>,
    cover_art_file: Option<CoverArtFile>,
    metadata_dirty: bool,
}

impl MediaSync {
    pub fn new(
        media_controls: Option<MediaControlsBackend>,
        #[cfg(target_os = "windows")] notifications: Option<NotificationsBackend>,
        #[cfg(not(target_os = "windows"))] _notifications: Option<std::convert::Infallible>,
    ) -> Self {
        Self {
            media_controls,
            #[cfg(target_os = "windows")]
            notifications,
            cover_art_file: None,
            metadata_dirty: false,
        }
    }

    pub fn clear(&mut self) {
        self.cover_art_file = None;
        self.metadata_dirty = true;
    }

    /// Pump platform messages (Windows SMTC). Call every frame.
    pub fn pump_messages(&self) {
        if let Some(mc) = &self.media_controls {
            mc.pump_messages();
        }
    }

    /// Push current playback state to OS media controls. Call every frame.
    pub fn sync_state(&mut self, playback: &PlaybackController) {
        if let Some(track) = &playback.playing_track {
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
                let status = if playback.is_playing() {
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

    pub fn on_track_changed(&mut self, track: &CurrentTrackInfo) {
        self.update_cover_art_file(Some(track));
        self.metadata_dirty = true;
    }

    fn update_cover_art_file(&mut self, track: Option<&CurrentTrackInfo>) {
        self.cover_art_file = None;
        if let Some(track) = track {
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
}
