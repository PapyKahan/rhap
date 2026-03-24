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
    #[cfg(not(target_os = "windows"))]
    Unsupported,
}

impl NotificationsTrait for NotificationsBackend {
    fn show_track_change(&self, _content: &NotificationContent) -> Result<()> {
        match self {
            #[cfg(target_os = "windows")]
            Self::WinRt(n) => n.show_track_change(_content),
            #[cfg(not(target_os = "windows"))]
            Self::Unsupported => Ok(()),
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
    Err(anyhow::anyhow!("Notifications not supported on this platform"))
}
