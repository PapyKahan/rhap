use anyhow::{anyhow, Result};
use log::{debug, error};
use wasapi::{ShareMode, calculate_period_100ns, Direction};
use std::sync::{
    mpsc::{sync_channel, Receiver, SyncSender},
    Arc, Condvar, Mutex,
};

use super::{stream::Streamer, com::com_initialize};
use crate::audio::{DeviceTrait, StreamParams, StreamingCommand};

pub struct Device {
    pub is_default: bool,
    pub(super) inner_device: wasapi::Device,
    pub(super) receiver: Option<Receiver<StreamingCommand>>,
}

impl Device {
    pub(super) fn new(inner_device: wasapi::Device, is_default: bool) -> Self {
        Self {
            inner_device,
            receiver: Option::None,
            is_default,
        }
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
        let mut streamer = Streamer::new(&self, rx, params)?;
        std::thread::spawn(move || -> Result<()> {
            streamer.start()
        });
        Ok(tx)
    }

    //fn set_status(&self, status: StreamingCommand) {
    //    let mut current_status = self.status.lock().expect("fail to lock mutex");
    //    match *current_status {
    //        StreamingCommand::Pause => {
    //            match status {
    //                StreamingCommand::Resume => self.pause_condition.notify_all(),
    //                _ => (),
    //            };
    //            *current_status = status
    //        }
    //        _ => *current_status = status,
    //    };
    //}

    //fn get_status(&self) -> StreamingCommand {
    //    *self.status.lock().expect("fail to lock mutex")
    //}

    fn stop(&mut self) -> Result<()> {
        if let Some(receiver) = self.receiver.take() {
            drop(receiver);
        }
        Ok(())
    }
}
