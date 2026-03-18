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
    WinRt(api::winrt::WinRtNotifications),
}

impl NotificationsTrait for NotificationsBackend {
    fn show_track_change(&self, content: &NotificationContent) -> Result<()> {
        match self {
            Self::WinRt(n) => n.show_track_change(content),
        }
    }
}

pub fn create_notifications() -> Result<NotificationsBackend> {
    let n = api::winrt::WinRtNotifications::new()?;
    Ok(NotificationsBackend::WinRt(n))
}
