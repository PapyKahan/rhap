use anyhow::{anyhow, Result};
use log::error;
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use symphonia::core::audio::RawSampleBuffer;
use symphonia::core::codecs::Decoder;
use symphonia::core::errors::Error;
use symphonia::core::formats::{FormatReader, SeekMode, SeekTo};
use symphonia::core::sample::i24;
use symphonia::core::units::Time;

use crate::audio::{
    BitsPerSample, Device, DeviceTrait, Host, HostTrait, StreamContext, StreamParams,
};
use crate::song::Song;

#[derive(Clone)]
pub struct Player {
    device: Device,
}

impl Player {
    pub fn new(host: Host, device_id: Option<u32>) -> Result<Self> {
        let device = host
            .create_device(device_id)
            .map_err(|err| anyhow!(err.to_string()))?;
        Ok(Player { device })
    }

    #[inline(always)]
    async fn fill_buffer(
        &self,
        decoder: Arc<Mutex<Box<dyn Decoder>>>,
        format: Arc<Mutex<Box<dyn FormatReader>>>,
        vec_buffer: Arc<Mutex<VecDeque<u8>>>,
        bits_per_sample: BitsPerSample,
    ) {
        tokio::spawn(async move {
            let mut format = format.lock().unwrap();
            let mut decoder = decoder.lock().unwrap();
            match format.seek(
                SeekMode::Accurate,
                SeekTo::Time {
                    time: Time::default(),
                    track_id: None,
                },
            ) {
                Ok(_) => (),
                Err(err) => {
                    error!("Error while seeking from the begining: {}", err);
                    return;
                }
            }
            decoder.reset();
            loop {
                let packet = match format.next_packet() {
                    Ok(packet) => packet,
                    Err(Error::ResetRequired) => {
                        unimplemented!();
                    }
                    Err(Error::IoError(err)) => {
                        // Error reading packet: IoError(Custom { kind: UnexpectedEof, error: "end of stream" })
                        match err.kind() {
                            std::io::ErrorKind::UnexpectedEof => {
                                break;
                            }
                            _ => {
                                error!("Error reading packet: {:?}", err);
                                break;
                            }
                        }
                    }
                    Err(err) => {
                        error!("Error reading packet: {:?}", err);
                        break;
                    }
                };

                // Consume any new metadata that has been read since the last packet.
                while !format.metadata().is_latest() {
                    format.metadata().pop();
                }

                match decoder.decode(&packet) {
                    Ok(_decoded) => {
                        let spec = *_decoded.spec();
                        let duration = _decoded.capacity() as u64;

                        // Not very efficient, but i can't create a RawSampleBuffer dynamically
                        // so i have to create one for each possible bits_per_sample and at eatch iteration
                        match bits_per_sample {
                            BitsPerSample::Bits8 => {
                                let mut sample_buffer = RawSampleBuffer::<u8>::new(duration, spec);
                                sample_buffer.copy_interleaved_ref(_decoded);
                                for i in sample_buffer.as_bytes().iter() {
                                    vec_buffer.lock().unwrap().push_back(*i);
                                }
                            }
                            BitsPerSample::Bits16 => {
                                let mut sample_buffer = RawSampleBuffer::<i16>::new(duration, spec);
                                sample_buffer.copy_interleaved_ref(_decoded);
                                for i in sample_buffer.as_bytes().iter() {
                                    vec_buffer.lock().unwrap().push_back(*i);
                                }
                            }
                            BitsPerSample::Bits24 => {
                                let mut sample_buffer = RawSampleBuffer::<i24>::new(duration, spec);
                                sample_buffer.copy_interleaved_ref(_decoded);
                                for i in sample_buffer.as_bytes().iter() {
                                    vec_buffer.lock().unwrap().push_back(*i);
                                }
                            }
                            BitsPerSample::Bits32 => {
                                let mut sample_buffer = RawSampleBuffer::<f32>::new(duration, spec);
                                sample_buffer.copy_interleaved_ref(_decoded);
                                for i in sample_buffer.as_bytes().iter() {
                                    vec_buffer.lock().unwrap().push_back(*i);
                                }
                            }
                        };
                    }
                    Err(Error::DecodeError(_)) => (),
                    Err(_) => break,
                }
            }
        });
        tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
    }

    /// Plays a FLAC file
    /// - params:
    ///    - song: song struct
    pub async fn play_song(&mut self, song: &Song) -> Result<()> {
        let decoder = song.decoder.clone();
        let format = song.format.clone();

        let buffer = Arc::new(Mutex::new(VecDeque::new()));
        self.fill_buffer(decoder, format, buffer.clone(), song.bits_per_sample)
            .await;

        let mut device = self.device.clone();
        let streamparams = StreamParams {
            samplerate: song.sample,
            channels: song.channels as u8,
            bits_per_sample: song.bits_per_sample,
            buffer_length: 0,
            exclusive: true,
        };
        tokio::spawn(async move {
            device
                .stream(StreamContext::new(buffer, streamparams))
                .map_err(|err| anyhow!(err.to_string()))?;
            Ok::<(), anyhow::Error>(())
        });

        Ok(())
    }

    pub fn stop(&self) {
        self.device.stop();
    }

    pub(crate) fn is_playing(&self) -> bool {
        self.device.is_playing()
    }
}
