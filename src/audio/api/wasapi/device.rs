use anyhow::{anyhow, Result};
use std::sync::mpsc::{sync_channel, Receiver, SyncSender};

use super::stream::Streamer;
use crate::audio::{DeviceTrait, StreamParams, StreamingCommand};

pub struct Device {
    pub is_default: bool,
    pub(super) inner_device: wasapi::Device,
    pub(super) receiver: Option<Receiver<StreamingCommand>>,
    pub(super) stream_thread_handle: Option<std::thread::JoinHandle<Result<()>>>,
}

impl Device {
    pub(super) fn new(inner_device: wasapi::Device, is_default: bool) -> Self {
        Self {
            inner_device,
            receiver: Option::None,
            is_default,
            stream_thread_handle: Option::None,
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
        self.stream_thread_handle = Some(std::thread::spawn(move || -> Result<()> {
            streamer.start()?;
            println!("streamer stopped");
            return Ok(());
        }));
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

    fn wait_till_ready(&mut self) -> Result<()> {
        if let Some(handle) = self.stream_thread_handle.take() {
            handle
                .join()
                .unwrap_or_else(|_| Ok(()))
                .map_err(|err| anyhow!(err.to_string()))
        } else {
            Ok(())
        }
    }
}
