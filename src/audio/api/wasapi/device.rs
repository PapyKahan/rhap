use std::error::Error;
use wasapi::WaveFormat;

use super::stream::Stream;
use crate::audio::{StreamTrait, DeviceTrait, StreamParams, Capabilities, SampleRate, api::wasapi::com::com_initialize};

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

    fn get_capabilities(&self) -> Result<Vec<Capabilities>, Box<dyn std::error::Error>> {
        com_initialize();
        let client = self.inner_device.get_iaudioclient()?;
        let capabilities = vec![];
        let format = WaveFormat::new(16, 16, &wasapi::SampleType::Int, SampleRate::Rate44100Hz as usize, 2, None);
        println!("{:?}", format);
        println!("Find a supported format");
        let supported = client.is_supported(&format, &wasapi::ShareMode::Exclusive)?;
        println!("Supported = {:?}", supported);
        Ok(capabilities)
    }

    fn build_stream(&self, params: StreamParams) -> Result<Box<dyn StreamTrait>, Box<dyn Error>>
    {
        let stream = Stream::build_from_device(&self, params)?;
        Ok(Box::new(stream))
    }
}
