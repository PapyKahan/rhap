use anyhow::{anyhow, Result};
use log::error;
use symphonia::core::audio::RawSampleBuffer;
use symphonia::core::errors::Error;
use symphonia::core::formats::{SeekMode, SeekTo};
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

    /// Plays a FLAC file
    /// - params:
    ///    - song: song struct
    pub async fn play_song(&mut self, song: &Song) -> Result<()> {
        song.format.lock().unwrap().seek(
            SeekMode::Accurate,
            SeekTo::Time {
                time: Time::default(),
                track_id: None,
            },
        )?;
        song.decoder.lock().unwrap().reset();
        let decoder = song.decoder.clone();
        let format = song.format.clone();
        let bits_per_sample = song.bits_per_sample;

        let streamparams = StreamParams {
            samplerate: song.sample,
            channels: song.channels as u8,
            bits_per_sample: song.bits_per_sample,
            buffer_length: 0,
            exclusive: true,
        };

        let mut device = self.device.clone();
        tokio::spawn(async move {
            device
                .stream(StreamContext::new(streamparams))
                .map_err(|err| anyhow!(err.to_string()))?;
            Ok::<(), anyhow::Error>(())
        });

        let device = self.device.clone();
        tokio::spawn(async move {
            loop {
                let packet = match format.lock().unwrap().next_packet() {
                    Ok(packet) => packet,
                    Err(Error::ResetRequired) => {
                        unimplemented!();
                    }
                    Err(Error::IoError(err)) => {
                        // Error reading packet: IoError(Custom { kind: UnexpectedEof, error: "end of stream" })
                        match err.kind() {
                            std::io::ErrorKind::UnexpectedEof => {
                                device.stop();
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

                match decoder.lock().unwrap().decode(&packet) {
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
                                    device.send(*i)?;
                                }
                            }
                            BitsPerSample::Bits16 => {
                                let mut sample_buffer = RawSampleBuffer::<i16>::new(duration, spec);
                                sample_buffer.copy_interleaved_ref(_decoded);
                                for i in sample_buffer.as_bytes().iter() {
                                    device.send(*i)?;
                                }
                            }
                            BitsPerSample::Bits24 => {
                                let mut sample_buffer = RawSampleBuffer::<i24>::new(duration, spec);
                                sample_buffer.copy_interleaved_ref(_decoded);
                                for i in sample_buffer.as_bytes().iter() {
                                    device.send(*i)?;
                                }
                            }
                            BitsPerSample::Bits32 => {
                                let mut sample_buffer = RawSampleBuffer::<f32>::new(duration, spec);
                                sample_buffer.copy_interleaved_ref(_decoded);
                                for i in sample_buffer.as_bytes().iter() {
                                    device.send(*i)?;
                                }
                            }
                        };
                    }
                    Err(Error::DecodeError(_)) => (),
                    Err(_) => break,
                }
            }

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
