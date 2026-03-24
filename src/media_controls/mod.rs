pub(crate) mod api;

use anyhow::Result;
use std::time::Duration;

use crate::action::Action;

/// Must be called before any COM initialization so SMTC can resolve the AUMID.
pub fn init_platform() {
    #[cfg(target_os = "windows")]
    unsafe {
        use windows::core::w;
        use windows::Win32::UI::Shell::SetCurrentProcessExplicitAppUserModelID;
        let _ = SetCurrentProcessExplicitAppUserModelID(w!("rhap"));
    }
}

pub struct TrackMetadata<'a> {
    pub title: &'a str,
    pub artist: &'a str,
    pub album: &'a str,
    pub duration: Option<Duration>,
    pub cover_url: Option<&'a str>,
}

pub enum PlaybackStatus {
    Playing,
    Paused,
    Stopped,
}

pub trait MediaControlsTrait {
    fn set_metadata(&mut self, metadata: &TrackMetadata) -> Result<()>;
    fn set_playback(&mut self, status: PlaybackStatus) -> Result<()>;
}

pub enum MediaControlsBackend {
    Souvlaki(api::souvlaki::SouvlakiMediaControls),
}

impl MediaControlsBackend {
    pub fn pump_messages(&self) {
        match self {
            Self::Souvlaki(c) => c.pump_messages(),
        }
    }
}

impl MediaControlsTrait for MediaControlsBackend {
    fn set_metadata(&mut self, metadata: &TrackMetadata) -> Result<()> {
        match self {
            Self::Souvlaki(c) => c.set_metadata(metadata),
        }
    }
    fn set_playback(&mut self, status: PlaybackStatus) -> Result<()> {
        match self {
            Self::Souvlaki(c) => c.set_playback(status),
        }
    }
}

pub struct MediaControlsHandle {
    pub backend: MediaControlsBackend,
    pub event_rx: std::sync::mpsc::Receiver<Action>,
}

pub fn create_media_controls() -> Result<MediaControlsHandle> {
    let (controls, rx) = api::souvlaki::SouvlakiMediaControls::new()?;
    Ok(MediaControlsHandle {
        backend: MediaControlsBackend::Souvlaki(controls),
        event_rx: rx,
    })
}
