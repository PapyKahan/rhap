use std::sync::Arc;

use super::stream::Streamer;
use crate::audio::{DeviceTrait, StreamContext};

#[derive(Clone)]
pub struct Device {
    pub is_default: bool,
    pub(super) inner_device: Arc<wasapi::Device>,
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

    fn stream(&self, context: StreamContext) -> Result<(), Box<dyn std::error::Error>> {
        let mut streamer = Streamer::new(&self, context)?;
        streamer.start()
    }
}
