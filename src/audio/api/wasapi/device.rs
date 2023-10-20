use std::sync::{Arc, Mutex, Condvar, mpsc::{channel, Receiver, Sender, sync_channel, SyncSender}};

use super::stream::Streamer;
use crate::audio::{DeviceTrait, StreamContext, PlaybackCommand};

#[derive(Clone)]
pub struct Device {
    pub is_default: bool,
    status: Arc<Mutex<PlaybackCommand>>,
    pub(super) wait_condition: Arc<Condvar>,
    pub(super) inner_device: Arc<wasapi::Device>,
    pub(super) receiver: Arc<Receiver<u8>>,
    sender: Arc<SyncSender<u8>>,
}

impl Device {
    pub(super) fn new(inner_device: wasapi::Device, is_default: bool) -> Self {
        let (tx, rx) = sync_channel::<u8>(4096);
        Self {
            inner_device: Arc::new(inner_device),
            receiver: Arc::new(rx),
            sender: Arc::new(tx),
            is_default,
            status: Arc::new(Mutex::new(PlaybackCommand::Stop)),
            wait_condition: Arc::new(Condvar::new())
        }
    }

    pub(super) fn wait_readiness(&self) {
        let status = self.status.lock().expect("fail to lock status mutex");
        let _ = self.wait_condition.wait(status);
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
        self.set_status(PlaybackCommand::Play);
        streamer.start()?;
        self.set_status(PlaybackCommand::Stop);
        Ok(())
    }

    fn set_status(&self, status: PlaybackCommand) {
        let mut current_status = self.status.lock().expect("fail to lock mutex");
        match *current_status {
            PlaybackCommand::Pause => {
                match status {
                    PlaybackCommand::Play => self.wait_condition.notify_all(),
                    _ => ()
                };
                *current_status = status
            },
            _ => *current_status = status
        };
    }

    fn is_playing(&self) -> bool {
        match *self.status.lock().expect("fail to lock mutex") {
            PlaybackCommand::Stop => false,
            _ => true
        }
    }

    fn get_status(&self) -> PlaybackCommand {
        *self.status.lock().expect("fail to lock mutex")
    }

    fn stop(&self) {
        self.set_status(PlaybackCommand::Stop)
    }

    fn send(&self, i: u8) -> Result<(), std::sync::mpsc::SendError<u8>> {
        self.sender.send(i)
    }
}
