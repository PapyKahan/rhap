#[cfg(unix)]
use jack::{Client, ClientOptions, Control, Port, ProcessHandler, ProcessScope};
#[cfg(unix)]
use std::sync::{Arc, Mutex as StdMutex};
#[cfg(unix)]
use anyhow::{Result, anyhow};
#[cfg(unix)]
use std::collections::VecDeque;
#[cfg(unix)]
use crate::audio::{StreamingData, BitsPerSample};

#[cfg(unix)]
pub struct JackClient {
    client: Option<Client>,
    active_client: Option<jack::AsyncClient<(), JackProcessHandler>>,
    output_port: Option<Port<jack::AudioOut>>,
    output_port_name: String,
    audio_buffer: Arc<StdMutex<VecDeque<f32>>>,
    sample_rate: u32,
    buffer_size: u32,
    channels: u8,
    bits_per_sample: BitsPerSample,
}

#[cfg(unix)]
impl JackClient {
    pub fn new(client_name: &str, _high_priority: bool, channels: u8, bits_per_sample: BitsPerSample) -> Result<Self> {
        let (client, _status) = Client::new(client_name, ClientOptions::NO_START_SERVER)?;
        
        let sample_rate = client.sample_rate();
        let buffer_size = client.buffer_size();
        
        let output_port = client.register_port("output", jack::AudioOut::default())?;
        let output_port_name = format!("{}:output", client_name);
        
        // Create a shared buffer for audio samples
        let audio_buffer = Arc::new(StdMutex::new(VecDeque::new()));

        Ok(Self {
            client: Some(client),
            active_client: None,
            output_port: Some(output_port),
            output_port_name,
            audio_buffer,
            sample_rate: sample_rate.try_into().unwrap(),
            buffer_size,
            channels,
            bits_per_sample,
        })
    }

    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    pub fn buffer_size(&self) -> u32 {
        self.buffer_size
    }

    pub fn get_audio_buffer(&self) -> Arc<StdMutex<VecDeque<f32>>> {
        self.audio_buffer.clone()
    }

    pub fn activate(&mut self) -> Result<()> {
        let audio_buffer = self.audio_buffer.clone();
        let output_port = self.output_port.take()
            .ok_or_else(|| anyhow!("Output port already taken"))?;

        let process_handler = JackProcessHandler {
            audio_buffer,
            output_port,
        };

        let client = self.client.take()
            .ok_or_else(|| anyhow!("Client already taken"))?;

        let active_client = client.activate_async((), process_handler)?;
        
        // Store the active client to keep it alive
        self.active_client = Some(active_client);
        
        // Connect output port to system playback ports
        self.connect_to_system_playback()?;
        
        Ok(())
    }
    
    fn connect_to_system_playback(&self) -> Result<()> {
        if let Some(ref active_client) = self.active_client {
            let client = active_client.as_client();
            
            // Get available playback ports (speakers/headphones)
            let playback_ports = client.ports(Some("system:playback.*"), None, jack::PortFlags::IS_INPUT);
            
            if !playback_ports.is_empty() {
                // Connect to the first available playback port (left channel)
                let connection_result = client.connect_ports_by_name(&self.output_port_name, &playback_ports[0]);
                if let Err(e) = connection_result {
                    eprintln!("Warning: Could not connect to system playback port: {}", e);
                    // Don't fail here, as JACK might still work with manual connections
                }
            } else {
                eprintln!("Warning: No system playback ports found. You may need to manually connect in JACK.");
            }
        }
        Ok(())
    }

    pub fn start_processing_data(&self, mut receiver: tokio::sync::mpsc::Receiver<StreamingData>) -> tokio::task::JoinHandle<()> {
        let audio_buffer = self.audio_buffer.clone();
        let bits_per_sample = self.bits_per_sample;
        let channels = self.channels;
        let buffer_size = self.buffer_size as usize;
        
        tokio::spawn(async move {
            let mut byte_buffer = Vec::new();
            let mut converted_samples = Vec::new();
            let bytes_per_sample = (bits_per_sample as usize) / 8;
            let frame_size = bytes_per_sample * channels as usize;
            
            // Target buffer size - aim for roughly 4 Jack periods worth of samples
            let target_samples = buffer_size * 4;
            
            while let Some(streaming_data) = receiver.recv().await {
                match streaming_data {
                    StreamingData::Data(byte) => {
                        byte_buffer.push(byte);
                        
                        // Convert accumulated bytes to f32 samples when we have enough data
                        while byte_buffer.len() >= frame_size {
                            let frame_bytes: Vec<u8> = byte_buffer.drain(0..frame_size).collect();
                            
                            // Convert bytes to f32 sample (taking first channel only)
                            // Note: Player uses to_ne_bytes(), so we should use from_ne_bytes()
                            let sample = match bits_per_sample {
                                BitsPerSample::Bits16 => {
                                    let sample_bytes = [frame_bytes[0], frame_bytes[1]];
                                    let sample_i16 = i16::from_ne_bytes(sample_bytes);
                                    sample_i16 as f32 / i16::MAX as f32
                                }
                                BitsPerSample::Bits24 => {
                                    let bytes = [frame_bytes[0], frame_bytes[1], frame_bytes[2], 0u8];
                                    let sample_i32 = i32::from_ne_bytes(bytes);
                                    (sample_i32 >> 8) as f32 / ((1 << 23) as f32)
                                }
                                BitsPerSample::Bits32 => {
                                    f32::from_ne_bytes([frame_bytes[0], frame_bytes[1], frame_bytes[2], frame_bytes[3]])
                                }
                            };
                            
                            converted_samples.push(sample);
                            
                            // Batch transfer samples to audio buffer when we have enough
                            if converted_samples.len() >= target_samples {
                                if let Ok(mut buffer) = audio_buffer.try_lock() {
                                    buffer.extend(converted_samples.drain(..));
                                }
                            }
                        }
                        
                        // Also transfer remaining samples periodically to prevent starvation
                        if !converted_samples.is_empty() && converted_samples.len() >= buffer_size {
                            if let Ok(mut buffer) = audio_buffer.try_lock() {
                                buffer.extend(converted_samples.drain(..));
                            }
                        }
                    }
                    StreamingData::EndOfStream => {
                        // Transfer any remaining samples
                        if !converted_samples.is_empty() {
                            if let Ok(mut buffer) = audio_buffer.try_lock() {
                                buffer.extend(converted_samples.drain(..));
                            }
                        }
                        break;
                    }
                }
            }
        })
    }
}

#[cfg(unix)]
struct JackProcessHandler {
    audio_buffer: Arc<StdMutex<VecDeque<f32>>>,
    output_port: Port<jack::AudioOut>,
}

#[cfg(unix)]
impl ProcessHandler for JackProcessHandler {
    fn process(&mut self, _client: &Client, ps: &ProcessScope) -> Control {
        let output = self.output_port.as_mut_slice(ps);
        
        // Try to lock and read from audio buffer (non-blocking)
        if let Ok(mut buffer) = self.audio_buffer.try_lock() {
            // Read samples from buffer
            for sample in output.iter_mut() {
                if let Some(audio_sample) = buffer.pop_front() {
                    *sample = audio_sample;
                } else {
                    *sample = 0.0; // Silence if no data
                }
            }
        } else {
            // If we can't lock the buffer (highly unlikely in practice), fill with silence
            for sample in output.iter_mut() {
                *sample = 0.0;
            }
        }
        
        Control::Continue
    }
}

#[cfg(not(unix))]
pub struct JackClient;

#[cfg(not(unix))]
impl JackClient {
    pub fn new(_client_name: &str, _high_priority: bool) -> Result<Self> {
        Err(anyhow::anyhow!("JACK is only supported on Unix systems"))
    }
}