use crate::audio::{DeviceTrait, Capabilities, StreamParams, StreamingData, SampleRate, BitsPerSample};
use anyhow::{Result, anyhow};
use tokio::sync::mpsc::{self, Sender, Receiver};
use std::sync::Arc;
use tokio::sync::Mutex;

#[cfg(unix)]
use super::api::JackClient;
#[cfg(unix)]
use ringbuf::{HeapProd, traits::Producer};

pub struct Device {
    name: String,
    high_priority_mode: bool,
    #[cfg(unix)]
    jack_client: Option<JackClient>,
    streaming_task: Option<tokio::task::JoinHandle<()>>,
    sender: Option<Sender<StreamingData>>,
}

impl Device {
    pub fn new(name: &str, high_priority_mode: bool) -> Result<Self> {
        Ok(Self {
            name: name.to_string(),
            high_priority_mode,
            #[cfg(unix)]
            jack_client: None,
            streaming_task: None,
            sender: None,
        })
    }
}

impl DeviceTrait for Device {
    fn is_default(&self) -> Result<bool> {
        Ok(self.name == "default")
    }

    fn name(&self) -> Result<String> {
        Ok(format!("JACK: {}", self.name))
    }

    fn get_capabilities(&self) -> Result<Capabilities> {
        #[cfg(unix)]
        {
            // JACK capabilities depend on the server configuration
            // We'll provide common sample rates and bit depths
            Ok(Capabilities {
                sample_rates: vec![
                    SampleRate::Rate44100Hz,
                    SampleRate::Rate48000Hz,
                    SampleRate::Rate96000Hz,
                    SampleRate::Rate192000Hz,
                ],
                bits_per_samples: vec![
                    BitsPerSample::Bits16,
                    BitsPerSample::Bits24,
                    BitsPerSample::Bits32,
                ],
            })
        }
        
        #[cfg(not(unix))]
        {
            Err(anyhow!("JACK is only supported on Unix systems"))
        }
    }

    fn start(&mut self, params: &StreamParams) -> Result<Sender<StreamingData>> {
        #[cfg(unix)]
        {
            if self.jack_client.is_some() {
                return Err(anyhow!("Device is already started"));
            }

            // Create JACK client
            let mut jack_client = JackClient::new("rhap_player", self.high_priority_mode)?;
            let producer = jack_client.get_producer();
            
            // Activate JACK client
            jack_client.activate()?;
            
            // Create channel for streaming data
            let (tx, rx) = mpsc::channel::<StreamingData>(8192);
            
            // Start background task to pump data from channel to ring buffer
            let streaming_task = tokio::spawn(Self::streaming_task(
                rx,
                producer,
                params.channels,
                params.bits_per_sample,
            ));

            self.jack_client = Some(jack_client);
            self.streaming_task = Some(streaming_task);
            self.sender = Some(tx.clone());

            Ok(tx)
        }
        
        #[cfg(not(unix))]
        {
            Err(anyhow!("JACK is only supported on Unix systems"))
        }
    }

    fn pause(&mut self) -> Result<()> {
        // For now, we'll handle pause by not sending data
        // A more sophisticated implementation could pause the JACK client
        Ok(())
    }

    fn resume(&mut self) -> Result<()> {
        // Resume by continuing to send data
        Ok(())
    }

    fn stop(&mut self) -> Result<()> {
        #[cfg(unix)]
        {
            if let Some(task) = self.streaming_task.take() {
                task.abort();
            }
            
            self.jack_client = None;
            self.sender = None;
            
            Ok(())
        }
        
        #[cfg(not(unix))]
        {
            Ok(())
        }
    }
}

#[cfg(unix)]
impl Device {
    async fn streaming_task(
        mut rx: Receiver<StreamingData>,
        producer: Arc<Mutex<Option<HeapProd<f32>>>>,
        channels: u8,
        bits_per_sample: BitsPerSample,
    ) {
        let mut audio_buffer = Vec::new();
        
        while let Some(data) = rx.recv().await {
            match data {
                StreamingData::Data(byte) => {
                    audio_buffer.push(byte);
                    
                    // Convert bytes to f32 samples when we have enough data
                    let bytes_per_sample = (bits_per_sample as usize) / 8;
                    let frame_size = bytes_per_sample * channels as usize;
                    
                    while audio_buffer.len() >= frame_size {
                        // Convert bytes to f32 sample
                        let sample = match bits_per_sample {
                            BitsPerSample::Bits16 => {
                                if audio_buffer.len() >= 2 {
                                    let bytes = [audio_buffer.remove(0), audio_buffer.remove(0)];
                                    let sample_i16 = i16::from_le_bytes(bytes);
                                    sample_i16 as f32 / i16::MAX as f32
                                } else {
                                    break;
                                }
                            }
                            BitsPerSample::Bits24 => {
                                if audio_buffer.len() >= 3 {
                                    let bytes = [
                                        audio_buffer.remove(0),
                                        audio_buffer.remove(0),
                                        audio_buffer.remove(0),
                                        0u8, // Pad to 32-bit
                                    ];
                                    let sample_i32 = i32::from_le_bytes(bytes);
                                    (sample_i32 >> 8) as f32 / ((1 << 23) as f32)
                                } else {
                                    break;
                                }
                            }
                            BitsPerSample::Bits32 => {
                                if audio_buffer.len() >= 4 {
                                    let bytes = [
                                        audio_buffer.remove(0),
                                        audio_buffer.remove(0),
                                        audio_buffer.remove(0),
                                        audio_buffer.remove(0),
                                    ];
                                    let sample_f32 = f32::from_le_bytes(bytes);
                                    sample_f32
                                } else {
                                    break;
                                }
                            }
                        };
                        
                        // Send sample to ring buffer (handle multi-channel by taking first channel for now)
                        if let Ok(mut producer_guard) = producer.try_lock() {
                            if let Some(ref mut producer_rb) = *producer_guard {
                                let _ = producer_rb.try_push(sample);
                            }
                        }
                        
                        // Skip other channels for now (mono output)
                        for _ in 1..channels {
                            for _ in 0..bytes_per_sample {
                                if !audio_buffer.is_empty() {
                                    audio_buffer.remove(0);
                                }
                            }
                        }
                    }
                }
                StreamingData::EndOfStream => {
                    break;
                }
            }
        }
    }
}