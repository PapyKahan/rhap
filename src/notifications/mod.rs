pub(crate) mod api;

use anyhow::Result;

pub struct NotificationContent<'a> {
    pub title: &'a str,
    pub artist: &'a str,
    pub album: &'a str,
    pub cover_art_path: Option<&'a str>,
}

pub trait NotificationsTrait {
    fn show_track_change(&self, content: &NotificationContent) -> Result<()>;
}

pub enum NotificationsBackend {
    #[cfg(target_os = "windows")]
    WinRt(api::winrt::WinRtNotifications),
    // A zero-sized placeholder variant so the enum is never empty,
    // allowing match to remain exhaustive even when no real variants exist.
    #[cfg(not(target_os = "windows"))]
    _Unsupported(std::convert::Infallible),
}

impl NotificationsTrait for NotificationsBackend {
    fn show_track_change(&self, content: &NotificationContent) -> Result<()> {
        match self {
            #[cfg(target_os = "windows")]
            Self::WinRt(n) => n.show_track_change(content),
            #[cfg(not(target_os = "windows"))]
            Self::_Unsupported(infallible) => match *infallible {},
        }
    }
}

pub fn create_notifications() -> Result<NotificationsBackend> {
    #[cfg(target_os = "windows")]
    {
        let n = api::winrt::WinRtNotifications::new()?;
        return Ok(NotificationsBackend::WinRt(n));
    }
    #[cfg(not(target_os = "windows"))]
    {
        anyhow::bail!("Notifications not supported on this platform")
    }
}
