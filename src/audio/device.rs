use super::{api, BufferConfig, Capabilities, StreamParams};
use anyhow::{anyhow, Result};
use ringbuf::HeapProd;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;

pub struct BufferSignal {
    mutex: Mutex<()>,
    condvar: Condvar,
}

impl BufferSignal {
    pub fn new() -> Self {
        Self {
            mutex: Mutex::new(()),
            condvar: Condvar::new(),
        }
    }

    pub fn notify(&self) {
        self.condvar.notify_all();
    }

    pub fn wait_timeout(&self, timeout: Duration) {
        let guard = self.mutex.lock().unwrap();
        let _ = self.condvar.wait_timeout(guard, timeout);
    }
}

pub struct AudioPipeline {
    pub producer: HeapProd<u8>,
    pub end_of_stream: Arc<AtomicBool>,
    pub signal: Arc<BufferSignal>,
}

pub trait DeviceTrait: Send {
    fn is_default(&self) -> Result<bool>;
    fn name(&self) -> Result<String>;
    fn get_capabilities(&self) -> Result<Capabilities>;
    fn start(&mut self, params: &StreamParams, buffer: &BufferConfig) -> Result<AudioPipeline>;
    fn pause(&mut self) -> Result<()>;
    fn resume(&mut self) -> Result<()>;
    fn stop(&mut self) -> Result<()>;
}

pub enum Device {
    None,
    #[cfg(target_os = "windows")]
    Wasapi(api::wasapi::device::Device),
    #[cfg(target_os = "linux")]
    Alsa(api::alsa::device::Device),
    #[cfg(target_os = "linux")]
    PipeWire(api::pipewire::device::Device),
}

impl DeviceTrait for Device {
    fn is_default(&self) -> Result<bool> {
        match self {
            Self::None => Ok(false),
            #[cfg(target_os = "windows")]
            Self::Wasapi(device) => device.is_default(),
            #[cfg(target_os = "linux")]
            Self::Alsa(device) => device.is_default(),
            #[cfg(target_os = "linux")]
            Self::PipeWire(device) => device.is_default(),
        }
    }

    fn name(&self) -> Result<String> {
        match self {
            Self::None => Ok(String::from("none")),
            #[cfg(target_os = "windows")]
            Self::Wasapi(device) => device.name(),
            #[cfg(target_os = "linux")]
            Self::Alsa(device) => device.name(),
            #[cfg(target_os = "linux")]
            Self::PipeWire(device) => device.name(),
        }
    }

    fn get_capabilities(&self) -> Result<Capabilities> {
        match self {
            Self::None => Ok(Capabilities::all_possible()),
            #[cfg(target_os = "windows")]
            Self::Wasapi(device) => device.get_capabilities(),
            #[cfg(target_os = "linux")]
            Self::Alsa(device) => device.get_capabilities(),
            #[cfg(target_os = "linux")]
            Self::PipeWire(device) => device.get_capabilities(),
        }
    }

    fn start(&mut self, params: &StreamParams, buffer: &BufferConfig) -> Result<AudioPipeline> {
        match self {
            Self::None => Err(anyhow!("No host selected")),
            #[cfg(target_os = "windows")]
            Self::Wasapi(device) => device.start(params, buffer),
            #[cfg(target_os = "linux")]
            Self::Alsa(device) => device.start(params, buffer),
            #[cfg(target_os = "linux")]
            Self::PipeWire(device) => device.start(params, buffer),
        }
    }

    fn pause(&mut self) -> Result<()> {
        match self {
            Self::None => Ok(()),
            #[cfg(target_os = "windows")]
            Self::Wasapi(device) => device.pause(),
            #[cfg(target_os = "linux")]
            Self::Alsa(device) => device.pause(),
            #[cfg(target_os = "linux")]
            Self::PipeWire(device) => device.pause(),
        }
    }

    fn resume(&mut self) -> Result<()> {
        match self {
            Self::None => Ok(()),
            #[cfg(target_os = "windows")]
            Self::Wasapi(device) => device.resume(),
            #[cfg(target_os = "linux")]
            Self::Alsa(device) => device.resume(),
            #[cfg(target_os = "linux")]
            Self::PipeWire(device) => device.resume(),
        }
    }

    fn stop(&mut self) -> Result<()> {
        match self {
            Self::None => Ok(()),
            #[cfg(target_os = "windows")]
            Self::Wasapi(device) => device.stop(),
            #[cfg(target_os = "linux")]
            Self::Alsa(device) => device.stop(),
            #[cfg(target_os = "linux")]
            Self::PipeWire(device) => device.stop(),
        }
    }
}
