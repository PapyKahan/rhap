use std::error::Error;

use super::stream::Stream;
use crate::audio::{StreamTrait, DeviceTrait, StreamParams};

pub struct Device {
    pub is_default: bool,
    pub(super) inner_device: wasapi::Device,
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

    fn build_stream(&self, params: StreamParams) -> Result<Box<dyn StreamTrait>, Box<dyn Error>>
    {
        let stream = Stream::build_from_device(&self, params)?;
        Ok(Box::new(stream))
    }
}
