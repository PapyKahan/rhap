use anyhow::Result;
use std::time::Duration;
use tokio::sync::mpsc::Receiver;

use super::api::AudioClient;
//use super::api::EventHandle;
use super::api::ShareMode;
use super::api::ThreadPriority;
use super::api::WaveFormat;
use super::device::Device;
use crate::audio::StreamParams;
use crate::audio::StreamingData;

const REFTIMES_PER_MILLISEC: i64 = 10000;
//const REFTIMES_PER_SEC: i64 = 10000000;

pub struct Streamer {
    client: AudioClient,
    //eventhandle: EventHandle,
    format: WaveFormat,
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
        let format = WaveFormat::from(params);
        let sharemode = match params.exclusive {
            true => ShareMode::Exclusive,
            false => ShareMode::Shared,
        };

        let mut client = device.get_client(&params)?;

        client.initialize(&format, &sharemode)?;
        //let eventhandle = client.set_get_eventhandle()?;

        Ok(Streamer {
            client,
            //eventhandle,
            format,
            receiver,
        })
    }

    fn stop(&self) -> Result<()> {
        self.client.stop()
    }

    pub(crate) async fn start(&mut self) -> Result<()> {
        let _thread_priority = ThreadPriority::new()?;
        let mut buffer = vec![];
        let mut stream_started = false;
        let (mut available_buffer_in_frames, mut available_buffer_size) =
            self.client.get_available_buffer_size(&self.format)?;

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

                if !stream_started {
                    self.client.start()?;
                    stream_started = !stream_started;
                }

                //self.eventhandle.wait_for_event(1000)?;
                buffer.clear();
                loop {
                    (available_buffer_in_frames, available_buffer_size) =
                        self.client.get_available_buffer_size(&self.format)?;
                    if available_buffer_in_frames > 0 {
                        break;
                    }
                    tokio::time::sleep(Duration::from_millis(2)).await;
                }
            } else {
                let bytes_per_frames = self.format.get_block_align() as usize;
                let frames = buffer.len() / bytes_per_frames;
                self.client.write(buffer.as_slice())?;
                tokio::time::sleep(Duration::from_millis(
                    self.client.get_period() as u64 / REFTIMES_PER_MILLISEC as u64,
                ))
                .await;
                break;
            }
        }
        self.stop()
    }
}
