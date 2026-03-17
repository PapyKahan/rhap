use std::sync::mpsc;

use anyhow::{Context, Result};
use souvlaki::{MediaControlEvent, MediaControls, MediaMetadata, MediaPlayback, PlatformConfig};

use crate::action::Action;
use crate::media_controls::{MediaControlsTrait, PlaybackStatus, TrackMetadata};

pub struct SouvlakiMediaControls {
    controls: MediaControls,
    #[cfg(target_os = "windows")]
    _hwnd_guard: super::hwnd::HiddenWindow,
}

impl SouvlakiMediaControls {
    pub fn new() -> Result<(Self, mpsc::Receiver<Action>)> {
        #[cfg(target_os = "windows")]
        let hwnd_guard = super::hwnd::HiddenWindow::new()?;

        let config = PlatformConfig {
            dbus_name: "rhap",
            display_name: "rhap",
            #[cfg(target_os = "windows")]
            hwnd: Some(hwnd_guard.as_ptr()),
        };

        let mut controls =
            MediaControls::new(config).map_err(|e| anyhow::anyhow!("{}", e))?;

        let (tx, rx) = mpsc::channel();

        controls
            .attach(move |event: MediaControlEvent| {
                let action = match event {
                    MediaControlEvent::Play => Action::TogglePlayPause,
                    MediaControlEvent::Pause => Action::TogglePlayPause,
                    MediaControlEvent::Toggle => Action::TogglePlayPause,
                    MediaControlEvent::Next => Action::NextTrack,
                    MediaControlEvent::Previous => Action::PreviousTrack,
                    MediaControlEvent::Stop => Action::Stop,
                    _ => return,
                };
                let _ = tx.send(action);
            })
            .map_err(|e| anyhow::anyhow!("{}", e))
            .context("Failed to attach media controls callback")?;

        Ok((
            Self {
                controls,
                #[cfg(target_os = "windows")]
                _hwnd_guard: hwnd_guard,
            },
            rx,
        ))
    }
}

impl SouvlakiMediaControls {
    /// Pump platform messages (Windows only — no-op elsewhere).
    pub fn pump_messages(&self) {
        #[cfg(target_os = "windows")]
        self._hwnd_guard.pump_messages();
    }
}

impl MediaControlsTrait for SouvlakiMediaControls {
    fn set_metadata(&mut self, metadata: &TrackMetadata) -> Result<()> {
        self.controls
            .set_metadata(MediaMetadata {
                title: Some(metadata.title),
                artist: Some(metadata.artist),
                duration: metadata.duration,
                cover_url: metadata.cover_url,
                ..Default::default()
            })
            .map_err(|e| anyhow::anyhow!("{}", e))?;
        Ok(())
    }

    fn set_playback(&mut self, status: PlaybackStatus) -> Result<()> {
        let playback = match status {
            PlaybackStatus::Playing => MediaPlayback::Playing { progress: None },
            PlaybackStatus::Paused => MediaPlayback::Paused { progress: None },
            PlaybackStatus::Stopped => MediaPlayback::Stopped,
        };
        self.controls
            .set_playback(playback)
            .map_err(|e| anyhow::anyhow!("{}", e))?;
        Ok(())
    }
}
