use crate::audio::{DeviceTrait, Capabilities, StreamParams, StreamingData, SampleRate, BitsPerSample};
use anyhow::{Result, anyhow};
use tokio::sync::mpsc::{self, Sender};

#[cfg(unix)]
use super::api::JackClient;

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

            // Create JACK client with parameters
            let mut jack_client = JackClient::new("rhap_player", self.high_priority_mode, params.channels, params.bits_per_sample)?;
            
            // Create channel for streaming data
            let (tx, rx) = mpsc::channel::<StreamingData>(8192);
            
            // Start processing streaming data in background
            let processing_task = jack_client.start_processing_data(rx);
            
            // Activate JACK client
            jack_client.activate()?;

            self.jack_client = Some(jack_client);
            self.streaming_task = Some(processing_task);
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

