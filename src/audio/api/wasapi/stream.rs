use anyhow::Result;
use tokio::sync::mpsc::Receiver;

use super::api::AudioClient;
use super::api::ThreadPriority;
use super::device::Device;
use crate::audio::StreamParams;
use crate::audio::StreamingData;


pub struct Streamer {
    client: AudioClient,
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

        Ok(Streamer {
            client,
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

        let mut available_buffer_size = self.client.get_buffer_size();
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

                if !client_started {
                    self.client.start()?;
                    client_started = true;
                }

                available_buffer_size = self.client.get_available_buffer_size()?;
            } else {
                break;
            }
        }
        self.client.write(buffer.as_slice())?;
        self.stop()
    }
}
