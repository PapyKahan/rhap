use std::sync::{Arc, Mutex};

use super::stream::Streamer;
use crate::audio::{DeviceTrait, StreamContext, PlaybackStatus};

#[derive(Clone)]
pub struct Device {
    pub is_default: bool,
    status: Arc<Mutex<PlaybackStatus>>,
    pub(super) inner_device: Arc<wasapi::Device>,
}

impl Device {
    pub(super) fn new(inner_device: wasapi::Device, is_default: bool) -> Self {
        Self {
            inner_device: Arc::new(inner_device),
            is_default,
            status: Arc::new(Mutex::new(PlaybackStatus::Stoped))
        }
    }
}

unsafe impl Send for Device {}
unsafe impl Sync for Device {}

impl DeviceTrait for Device{
    fn is_default(&self) -> bool {
        self.is_default
    }

    fn name(&self) -> String {
        self.inner_device.get_friendlyname().unwrap_or_default()
    }

    fn stream(&mut self, context: StreamContext) -> Result<(), Box<dyn std::error::Error>> {
        let mut streamer = Streamer::new(&self, context)?;
        self.set_status(PlaybackStatus::Playing);
        streamer.start()?;
        self.set_status(PlaybackStatus::Stoped);
        Ok(())
    }

    fn set_status(&self, status: PlaybackStatus) {
        *self.status.lock().expect("fail to lock mutex") = status;
    }

    fn is_playing(&self) -> bool {
        match *self.status.lock().expect("fail to lock mutex") {
            PlaybackStatus::Stoped => false,
            PlaybackStatus::Paused => true,
            PlaybackStatus::Playing => true,
        }
    }

    fn get_status(&self) -> PlaybackStatus {
        *self.status.lock().expect("fail to lock mutex")
    }
}
