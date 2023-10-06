use anyhow::{anyhow, Result};
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use symphonia::core::audio::RawSampleBuffer;
use symphonia::core::codecs::{Decoder, DecoderOptions};
use symphonia::core::errors::Error;
use symphonia::core::formats::FormatReader;
use symphonia::core::io::MediaSourceStream;
use symphonia::core::probe::Hint;
use symphonia::core::sample::i24;

use crate::audio::{
    BitsPerSample, Device, DeviceTrait, Host, HostTrait, Stream, StreamFlow, StreamParams,
    StreamTrait,
};

#[derive(Clone)]
pub struct Player {
    device: Device,
    current_stream: Option<Stream>,
}

impl Player {
    pub fn new(host: Host, device_id: Option<u32>) -> Result<Self> {
        let device = host
            .create_device(device_id)
            .map_err(|err| anyhow!(err.to_string()))?;
        Ok(Player {
            device,
            current_stream: None
        })
    }

    #[inline(always)]
    async fn fill_buffer(
        &self,
        mut decoder: Box<dyn Decoder>,
        mut format: Box<dyn FormatReader>,
        vec_buffer: Arc<Mutex<VecDeque<u8>>>,
        bits_per_sample: BitsPerSample,
    ) {
        tokio::spawn(async move {
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
                                panic!("Error reading packet: {:?}", err);
                            }
                        }
                    }
                    Err(err) => {
                        println!("Error reading packet: {:?}", err);
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
    ///    - file: path to the FLAC file
    pub async fn play(&mut self, path: String) -> Result<()> {
        let source = std::fs::File::open(path.clone())?;
        let mss = MediaSourceStream::new(Box::new(source), Default::default());
        let hint = Hint::new();
        let meta_opts = Default::default();
        let fmt_opts = Default::default();
        let probed = symphonia::default::get_probe()
            .format(&hint, mss, &fmt_opts, &meta_opts)
            .expect("unsupported format");

        let format = probed.format;
        let track = format.tracks().get(0).unwrap();
        let samplerate = track.codec_params.sample_rate.unwrap();
        let channels = track.codec_params.channels.unwrap().count() as u8;
        let bits_per_sample = track.codec_params.bits_per_sample.unwrap_or(16) as u8;

        // Use the default options for the decoder.
        let dec_opts = DecoderOptions { verify: true };

        // Create a decoder for the track.
        let decoder = symphonia::default::get_codecs().make(&track.codec_params, &dec_opts)?;

        let vec_buffer = Arc::new(Mutex::new(VecDeque::new()));
        self.fill_buffer(
            decoder,
            format,
            vec_buffer.clone(),
            BitsPerSample::from(bits_per_sample),
        )
        .await;

        let streamparams = StreamParams {
            samplerate: samplerate.into(),
            channels,
            bits_per_sample: bits_per_sample.into(),
            buffer_length: 0,
            exclusive: true,
        };

        self.current_stream = None;
        let stream = self.device
            .build_stream(streamparams)
            .map_err(|err| anyhow!(err.to_string()))?;
            println!("Playing file path: {}", path);
            let callback = &mut |data: &mut [u8],
                                 buffer_size: usize|
             -> Result<StreamFlow, Box<dyn std::error::Error>> {
                let mut data_processing = StreamFlow::Continue;
                for i in 0..buffer_size {
                    if vec_buffer.lock().unwrap().is_empty() {
                        data_processing = StreamFlow::Complete;
                        break;
                    }
                    data[i] = vec_buffer.lock().unwrap().pop_front().unwrap_or_default();
                }
                Ok(data_processing)
            };
        self.current_stream = Some(stream);

        let mut stream = self.current_stream.as_ref().unwrap().clone();
        stream.start(callback).map_err(|err| anyhow!(err.to_string()))?;
        Ok(())
    }

    pub(crate) fn stop(&mut self) -> Result<()> {
        if self.current_stream.is_some() {
            println!("stoping");
            self.current_stream.as_mut().unwrap().stop().map_err(|err| anyhow!(err.to_string()))?;
        }
        Ok(())
    }
}
