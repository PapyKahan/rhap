use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::thread;
use symphonia::core::audio::RawSampleBuffer;
use symphonia::core::codecs::{Decoder, DecoderOptions};
use symphonia::core::errors::Error;
use symphonia::core::formats::{FormatOptions, FormatReader};
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;
use symphonia::core::sample::i24;

use crate::audio::api::wasapi::stream::Stream;
use crate::audio::{Device, StreamFlow, StreamParams, StreamTrait};

pub struct Player {
    device_id: u16,
}

impl Player {
    pub fn new(device_id: u16) -> Self {
        Player { device_id }
    }

    #[inline(always)]
    fn fill_buffer_32bits(
        &self,
        mut decoder: Box<dyn Decoder>,
        mut format: Box<dyn FormatReader>,
        vec_buffer: Arc<Mutex<VecDeque<u8>>>
    ) {
        thread::spawn(move || {
            let mut sample_buffer = None;
            loop {
                let packet = match format.next_packet() {
                    Ok(packet) => packet,
                    Err(Error::ResetRequired) => {
                        unimplemented!();
                    },
                    Err(Error::IoError(err)) => {
                        // Error reading packet: IoError(Custom { kind: UnexpectedEof, error: "end of stream" })
                        match err.kind() {
                            std::io::ErrorKind::UnexpectedEof => {
                                break;
                            },
                            _ => {
                                panic!("Error reading packet: {:?}", err);
                            }
                        }
                    },
                    Err(err) => {
                        println!("Error reading packet: {:?}", err);
                        break;
                    },
                };

                // Consume any new metadata that has been read since the last packet.
                while !format.metadata().is_latest() {
                    format.metadata().pop();
                }

                match decoder.decode(&packet) {
                    Ok(_decoded) => {
                        if sample_buffer.is_none() {
                            // Get the audio buffer specification.
                            let spec = *_decoded.spec();
                            // Get the capacity of the decoded buffer. Note: This is capacity, not length!
                            let duration = _decoded.capacity() as u64;
                            // Create the f32 sample buffer.
                            sample_buffer = Some(RawSampleBuffer::<i32>::new(duration, spec));
                        }

                        if let Some(buf) = &mut sample_buffer {
                            buf.copy_interleaved_ref(_decoded);
                            for i in buf.as_bytes().iter() {
                                vec_buffer.lock().unwrap().push_back(*i)
                            }
                        }
                    }
                    Err(Error::DecodeError(_)) => (),
                    Err(_) => break,
                }
            }
        });
        thread::sleep(std::time::Duration::from_secs(1));
    }

    #[inline(always)]
    fn fill_buffer_24bits(
        &self,
        mut decoder: Box<dyn Decoder>,
        mut format: Box<dyn FormatReader>,
        vec_buffer: Arc<Mutex<VecDeque<u8>>>
    ) {
        thread::spawn(move || {
            let mut sample_buffer = None;
            loop {
                let packet = match format.next_packet() {
                    Ok(packet) => packet,
                    Err(Error::ResetRequired) => {
                        unimplemented!();
                    },
                    Err(Error::IoError(err)) => {
                        // Error reading packet: IoError(Custom { kind: UnexpectedEof, error: "end of stream" })
                        match err.kind() {
                            std::io::ErrorKind::UnexpectedEof => {
                                break;
                            },
                            _ => {
                                panic!("Error reading packet: {:?}", err);
                            }
                        }
                    },
                    Err(err) => {
                        println!("Error reading packet: {:?}", err);
                        break;
                    },
                };

                // Consume any new metadata that has been read since the last packet.
                while !format.metadata().is_latest() {
                    format.metadata().pop();
                }

                match decoder.decode(&packet) {
                    Ok(_decoded) => {
                        if sample_buffer.is_none() {
                            // Get the audio buffer specification.
                            let spec = *_decoded.spec();
                            // Get the capacity of the decoded buffer. Note: This is capacity, not length!
                            let duration = _decoded.capacity() as u64;
                            // Create the f32 sample buffer.
                            sample_buffer = Some(RawSampleBuffer::<i24>::new(duration, spec));
                        }

                        if let Some(buf) = &mut sample_buffer {
                            buf.copy_interleaved_ref(_decoded);
                            for i in buf.as_bytes().iter() {
                                vec_buffer.lock().unwrap().push_back(*i)
                            }
                        }
                    }
                    Err(Error::DecodeError(_)) => (),
                    Err(_) => break,
                }
            }
        });
        thread::sleep(std::time::Duration::from_secs(1));
    }

    #[inline(always)]
    fn fill_buffer_16bits(
        &self,
        mut decoder: Box<dyn Decoder>,
        mut format: Box<dyn FormatReader>,
        vec_buffer: Arc<Mutex<VecDeque<u8>>>
    ) {
        thread::spawn(move || {
            let mut sample_buffer = None;
            loop {
                let packet = match format.next_packet() {
                    Ok(packet) => packet,
                    Err(Error::ResetRequired) => {
                        unimplemented!();
                    },
                    Err(Error::IoError(err)) => {
                        // Error reading packet: IoError(Custom { kind: UnexpectedEof, error: "end of stream" })
                        match err.kind() {
                            std::io::ErrorKind::UnexpectedEof => {
                                break;
                            },
                            _ => {
                                panic!("Error reading packet: {:?}", err);
                            }
                        }
                    },
                    Err(err) => {
                        println!("Error reading packet: {:?}", err);
                        break;
                    },
                };

                // Consume any new metadata that has been read since the last packet.
                while !format.metadata().is_latest() {
                    format.metadata().pop();
                }

                match decoder.decode(&packet) {
                    Ok(_decoded) => {
                        if sample_buffer.is_none() {
                            // Get the audio buffer specification.
                            let spec = *_decoded.spec();
                            // Get the capacity of the decoded buffer. Note: This is capacity, not length!
                            let duration = _decoded.capacity() as u64;
                            // Create the f32 sample buffer.
                            sample_buffer = Some(RawSampleBuffer::<i16>::new(duration, spec));
                        }

                        if let Some(buf) = &mut sample_buffer {
                            buf.copy_interleaved_ref(_decoded);
                            for i in buf.as_bytes().iter() {
                                vec_buffer.lock().unwrap().push_back(*i)
                            }
                        }
                    }
                    Err(Error::DecodeError(_)) => (),
                    Err(_) => break,
                }
            }
        });
        thread::sleep(std::time::Duration::from_secs(1));
    }

    /// Plays a FLAC file
    /// - params:
    ///    - file: path to the FLAC file
    pub fn play(&self, file: String) -> Result<(), String> {
        let src = std::fs::File::open(file.clone()).expect("failed to open media");
        let mss = MediaSourceStream::new(Box::new(src), Default::default());
        let hint = Hint::new();
        let meta_opts: MetadataOptions = Default::default();
        let fmt_opts: FormatOptions = Default::default();
        let probed = symphonia::default::get_probe()
            .format(&hint, mss, &fmt_opts, &meta_opts)
            .expect("unsupported format");

        let format = probed.format;
        let track = format.tracks().get(0).unwrap();
        let samplerate = track.codec_params.sample_rate.unwrap();
        let channels = track.codec_params.channels.unwrap().count() as u8;
        let bits_per_sample = track.codec_params.bits_per_sample.unwrap_or(16) as u8;

        // Use the default options for the decoder.
        let dec_opts: DecoderOptions = DecoderOptions { verify: true };

        // Create a decoder for the track.
        let decoder = symphonia::default::get_codecs()
            .make(&track.codec_params, &dec_opts)
            .expect("unsupported codec");

        let vec_buffer = Arc::new(Mutex::new(VecDeque::new()));
        match bits_per_sample {
            16 => self.fill_buffer_16bits(decoder, format, vec_buffer.clone()),
            24 => self.fill_buffer_24bits(decoder, format, vec_buffer.clone()),
            32 => self.fill_buffer_32bits(decoder, format, vec_buffer.clone()),
            _ => panic!("Unsupported bits per sample"),
        }

        let callback = move |data: &mut [u8], buffer_size: usize| -> Result<StreamFlow, String> {
            let mut data_processing = StreamFlow::Continue;
            for i in 0..buffer_size {
                if vec_buffer.lock().unwrap().is_empty() {
                    data_processing = StreamFlow::Complete;
                    break;
                }
                data[i] = vec_buffer.lock().unwrap().pop_front().unwrap();
            }
            Ok(data_processing)
        };

        let mut stream = match Stream::new(
            StreamParams {
                device: Device {
                    id: self.device_id,
                    name: String::from(""),
                },
                samplerate: samplerate.into(),
                channels,
                bits_per_sample: bits_per_sample.into(),
                buffer_length: 1000,
                exclusive: true,
            },
            callback,
        ) {
            Ok(s) => s,
            Err(e) => {
                return Err(format!("Failed to create stream: {}", e));
            }
        };

        println!("Playing file path: {}", file);
        match stream.start() {
            Ok(_) => {}
            Err(e) => return Err(format!("Failed to start stream: {}", e)),
        };

        match stream.stop() {
            Ok(_) => Ok(()),
            Err(e) => {
                return Err(format!("Failed to stop stream: {}", e));
            }
        }
    }
}
