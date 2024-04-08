use anyhow::Result;
use std::time::Duration;
use tokio::sync::mpsc::Receiver;

use super::api::AudioClient;
//use super::api::EventHandle;
use super::api::ThreadPriority;
use super::device::Device;
use crate::audio::StreamParams;
use crate::audio::StreamingData;

const REFTIMES_PER_MILLISEC: u64 = 10000;
const REFTIMES_PER_SEC: u64 = 10000000;

pub struct Streamer {
    client: AudioClient,
    //eventhandle: EventHandle,
    receiver: Receiver<StreamingData>,
}

unsafe impl Send for Streamer {}
unsafe impl Sync for Streamer {}

impl Streamer {
    pub(super) fn new(
        device: &Device,
        receiver: Receiver<StreamingData>,
        params: &StreamParams,
    ) -> Result<Self> {
        let mut client = device.get_client(params)?;
        client.initialize()?;
        //let eventhandle = client.set_get_eventhandle()?;

        Ok(Streamer {
            client,
            //eventhandle,
            receiver,
        })
    }

    fn stop(&self) -> Result<()> {
        self.client.stop()
    }

    pub(crate) async fn start(&mut self) -> Result<()> {
        let _thread_priority = ThreadPriority::new()?;
        let mut buffer = vec![];
        let mut client_started = false;

        let mut available_buffer_size = self.client.get_available_buffer_size()?;
        let samples_per_sec = self.client.get_samples_per_sec();
        let max_buffer_size = self.client.get_max_buffer_size();
        let actual_duration =
            REFTIMES_PER_SEC * self.client.get_max_buffer_frames() as u64 / samples_per_sec as u64;
        loop {
            if let Some(streaming_data) = self.receiver.recv().await {
                let data = match streaming_data {
                    StreamingData::Data(data) => data,
                    StreamingData::EndOfStream => break,
                };
                buffer.push(data);
                if buffer.len() != available_buffer_size {
                    continue;
                }

                self.client.write(buffer.as_slice())?;
                buffer.clear();

                //self.eventhandle.wait_for_event(1000)?;

                if !client_started {
                    self.client.start()?;
                    client_started = true;
                }

                loop {
                    available_buffer_size = self.client.get_available_buffer_size()?;
                    if available_buffer_size >= (max_buffer_size / 4) as usize {
                        break;
                    }
                    tokio::time::sleep(Duration::from_millis(1)).await;
                }
            } else {
                break;
            }
        }
        self.client.write(buffer.as_slice())?;
        loop {
            available_buffer_size = self.client.get_available_buffer_size()?;
            if available_buffer_size >= max_buffer_size {
                break;
            }
            tokio::time::sleep(Duration::from_millis(1)).await;
        }
        self.stop()
    }
}
