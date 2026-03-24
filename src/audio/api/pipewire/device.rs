use anyhow::Result;
use log::error;
use ringbuf::HeapRb;
use ringbuf::traits::Split;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use super::api::PwStreamHandle;
use crate::audio::device::{AudioPipeline, BufferSignal};
use crate::audio::{Capabilities, DeviceTrait, StreamParams};

pub struct Device {
    node_id: u32,
    description: String,
    is_default_device: bool,
    stream_handle: Option<PwStreamHandle>,
}

impl Device {
    pub fn new(
        node_id: u32,
        _node_name: String,
        description: String,
        is_default_device: bool,
    ) -> Self {
        Self {
            node_id,
            description,
            is_default_device,
            stream_handle: None,
        }
    }
}

// SAFETY: PipeWire operations for this device are confined to the audio output
// thread. The stream handle's command sender is Send. Device itself is not
// shared concurrently — it is owned by the Player and only accessed from the
// main/UI thread while the audio thread runs independently.
unsafe impl Send for Device {}
unsafe impl Sync for Device {}

impl DeviceTrait for Device {
    fn is_default(&self) -> Result<bool> {
        Ok(self.is_default_device)
    }

    fn name(&self) -> Result<String> {
        Ok(self.description.clone())
    }

    fn get_capabilities(&self) -> Result<Capabilities> {
        // PipeWire handles format conversion transparently; report all formats.
        Ok(Capabilities::all_possible())
    }

    fn start(&mut self, params: &StreamParams) -> Result<AudioPipeline> {
        self.stop()?;

        let bytes_per_frame = (params.bits_per_sample.0 / 8) as usize * params.channels as usize;
        // ~250ms at the given sample rate, minimum 64 KiB
        let ring_bytes = {
            let ms250 = (params.samplerate.0 as usize * bytes_per_frame * 250) / 1000;
            ms250.max(64 * 1024)
        };

        let ring = HeapRb::<u8>::new(ring_bytes);
        let (producer, consumer) = ring.split();

        let end_of_stream = Arc::new(AtomicBool::new(false));
        let signal = Arc::new(BufferSignal::new());

        let handle = super::api::start_stream(
            params,
            consumer,
            Arc::clone(&end_of_stream),
            Arc::clone(&signal),
            Some(self.node_id),
        )?;

        self.stream_handle = Some(handle);

        Ok(AudioPipeline {
            producer,
            end_of_stream,
            signal,
        })
    }

    fn pause(&mut self) -> Result<()> {
        if let Some(handle) = &self.stream_handle {
            handle.cork(true);
        }
        Ok(())
    }

    fn resume(&mut self) -> Result<()> {
        if let Some(handle) = &self.stream_handle {
            handle.cork(false);
        }
        Ok(())
    }

    fn stop(&mut self) -> Result<()> {
        if let Some(mut handle) = self.stream_handle.take() {
            handle.stop();
        }
        Ok(())
    }
}

impl Drop for Device {
    fn drop(&mut self) {
        if let Err(e) = self.stop() {
            error!("Error stopping PipeWire device on drop: {:#}", e);
        }
    }
}
