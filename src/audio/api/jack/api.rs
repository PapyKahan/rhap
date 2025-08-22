#[cfg(unix)]
use jack::{Client, ClientOptions, Control, Port, ProcessHandler, ProcessScope};
#[cfg(unix)]
use ringbuf::{HeapRb, traits::{Split, Consumer}, HeapProd, HeapCons};
#[cfg(unix)]
use std::sync::{Arc, Mutex as StdMutex};
#[cfg(unix)]
use tokio::sync::Mutex;
#[cfg(unix)]
use anyhow::{Result, anyhow};

#[cfg(unix)]
pub struct JackClient {
    client: Option<Client>,
    output_port: Option<Port<jack::AudioOut>>,
    consumer: Option<Arc<StdMutex<HeapCons<f32>>>>,
    producer: Arc<Mutex<Option<HeapProd<f32>>>>,
    sample_rate: u32,
    buffer_size: u32,
}

#[cfg(unix)]
impl JackClient {
    pub fn new(client_name: &str, _high_priority: bool) -> Result<Self> {
        let (client, _status) = Client::new(client_name, ClientOptions::NO_START_SERVER)?;
        
        let sample_rate = client.sample_rate();
        let buffer_size = client.buffer_size();
        
        // Create ring buffer - size it for ~100ms of audio
        let buffer_samples = (sample_rate as usize * 100) / 1000; // 100ms buffer
        let ring_buffer = HeapRb::<f32>::new(buffer_samples);
        let (producer, consumer) = ring_buffer.split();
        
        let output_port = client.register_port("output", jack::AudioOut::default())?;

        Ok(Self {
            client: Some(client),
            output_port: Some(output_port),
            consumer: Some(Arc::new(StdMutex::new(consumer))),
            producer: Arc::new(Mutex::new(Some(producer))),
            sample_rate: sample_rate.try_into().unwrap(),
            buffer_size,
        })
    }

    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    pub fn buffer_size(&self) -> u32 {
        self.buffer_size
    }

    pub fn get_producer(&self) -> Arc<Mutex<Option<HeapProd<f32>>>> {
        self.producer.clone()
    }

    pub fn activate(&mut self) -> Result<()> {
        let consumer = self.consumer.take()
            .ok_or_else(|| anyhow!("Consumer already taken"))?;
        
        let output_port = self.output_port.take()
            .ok_or_else(|| anyhow!("Output port already taken"))?;

        let process_handler = JackProcessHandler {
            consumer,
            output_port,
        };

        let client = self.client.take()
            .ok_or_else(|| anyhow!("Client already taken"))?;

        let _active_client = client.activate_async((), process_handler)?;
        
        Ok(())
    }
}

#[cfg(unix)]
struct JackProcessHandler {
    consumer: Arc<StdMutex<HeapCons<f32>>>,
    output_port: Port<jack::AudioOut>,
}

#[cfg(unix)]
impl ProcessHandler for JackProcessHandler {
    fn process(&mut self, _client: &Client, ps: &ProcessScope) -> Control {
        let output = self.output_port.as_mut_slice(ps);
        
        // Try to lock and read from ring buffer (non-blocking)
        if let Ok(mut consumer) = self.consumer.try_lock() {
            // Read samples from ring buffer, one by one
            for sample in output.iter_mut() {
                if let Some(audio_sample) = consumer.try_pop() {
                    *sample = audio_sample;
                } else {
                    *sample = 0.0; // Silence if no data
                }
            }
        } else {
            // If we can't lock the consumer (highly unlikely in practice), fill with silence
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