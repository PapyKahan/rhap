use anyhow::Result;
use std::sync::{
    mpsc::{sync_channel, Receiver, SyncSender},
    Arc, Condvar, Mutex,
};

use super::stream::Streamer;
use crate::audio::{DeviceTrait, StreamParams, StreamingCommand};

#[derive(Clone)]
pub struct Device {
    pub is_default: bool,
    status: Arc<Mutex<StreamingCommand>>,
    pub(super) pause_condition: Arc<Condvar>,
    pub(super) inner_device: Arc<wasapi::Device>,
    pub(super) receiver: Option<Arc<Receiver<StreamingCommand>>>,
}

impl Device {
    pub(super) fn new(inner_device: wasapi::Device, is_default: bool) -> Self {
        Self {
            inner_device: Arc::new(inner_device),
            receiver: Option::None,
            is_default,
            status: Arc::new(Mutex::new(StreamingCommand::None)),
            pause_condition: Arc::new(Condvar::new()),
        }
    }

    pub(super) fn wait_readiness(&self) {
        let status = self.status.lock().expect("fail to lock status mutex");
        let _ = self.pause_condition.wait(status);
    }
}

unsafe impl Send for Device {}
unsafe impl Sync for Device {}

impl DeviceTrait for Device {
    fn is_default(&self) -> bool {
        self.is_default
    }

    fn name(&self) -> String {
        self.inner_device.get_friendlyname().unwrap_or_default()
    }

    fn start(&mut self, params: StreamParams) -> Result<SyncSender<StreamingCommand>> {
        let (tx, rx) = sync_channel::<StreamingCommand>(4096);
        self.receiver = Option::Some(Arc::new(rx));
        let device = self.clone();
        std::thread::spawn(move || -> Result<()> {
            let mut streamer = Streamer::new(&device, params)?;
            streamer.start()
        });
        Ok(tx)
    }

    fn set_status(&self, status: StreamingCommand) {
        let mut current_status = self.status.lock().expect("fail to lock mutex");
        match *current_status {
            StreamingCommand::Pause => {
                match status {
                    StreamingCommand::Resume => self.pause_condition.notify_all(),
                    _ => (),
                };
                *current_status = status
            }
            _ => *current_status = status,
        };
    }

    fn get_status(&self) -> StreamingCommand {
        *self.status.lock().expect("fail to lock mutex")
    }

    fn stop(&mut self) -> Result<()> {
        if let Some(receiver) = self.receiver.take() {
            drop(receiver);
        }
        Ok(())
    }
}
