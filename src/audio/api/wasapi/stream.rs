use anyhow::Ok;
use anyhow::Result;
use tokio::sync::mpsc::Receiver;

use super::api::AudioClient;
use crate::audio::StreamingData;


pub struct Streamer {
    client: AudioClient,
    receiver: Receiver<StreamingData>,
    high_priority_mode: bool,
}

impl Streamer {
    pub(crate) async fn start(&mut self) -> Result<()> {
        Ok(())
    }
}
