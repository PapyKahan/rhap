use anyhow::Result;
use windows::core::HSTRING;
use windows::Data::Xml::Dom::XmlDocument;
use windows::UI::Notifications::{ToastNotification, ToastNotificationManager, ToastNotifier};

use crate::notifications::{NotificationContent, NotificationsTrait};

pub struct WinRtNotifications {
    notifier: ToastNotifier,
}

impl WinRtNotifications {
    pub fn new() -> Result<Self> {
        let app_id = HSTRING::from("rhap");
        let notifier = ToastNotificationManager::CreateToastNotifierWithId(&app_id)?;
        Ok(Self { notifier })
    }
}

fn xml_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            _ => out.push(c),
        }
    }
    out
}

impl NotificationsTrait for WinRtNotifications {
    fn show_track_change(&self, content: &NotificationContent) -> Result<()> {
        let title = xml_escape(content.title);
        let artist = xml_escape(content.artist);
        let album = xml_escape(content.album);

        let image_element = match content.cover_art_path {
            Some(path) => format!(
                r#"<image placement="appLogoOverride" src="{}" />"#,
                xml_escape(path)
            ),
            None => String::new(),
        };

        let xml = format!(
            r#"<toast>
  <visual>
    <binding template="ToastGeneric">
      {image_element}
      <text>{title}</text>
      <text>{artist} — {album}</text>
    </binding>
  </visual>
  <audio silent="true" />
</toast>"#
        );

        let doc = XmlDocument::new()?;
        doc.LoadXml(&HSTRING::from(&xml))?;
        let toast = ToastNotification::CreateToastNotification(&doc)?;
        self.notifier.Show(&toast)?;

        Ok(())
    }
}
