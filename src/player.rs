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

use crate::audio::api::wasapi::host::Host;
use crate::audio::{BitsPerSample, DeviceTrait, StreamFlow, StreamParams, StreamTrait};

pub struct Player {
    device_id: Option<u32>,
    host: Host,
    current_stream: Option<Box<dyn StreamTrait>>,
}

impl Player {
    pub fn new(device_id: Option<u32>) -> Result<Self, String> {
        Ok(Player {
            device_id,
            host: Host::new()?,
            current_stream : None
        })
    }

    #[inline(always)]
    fn fill_buffer(
        &self,
        mut decoder: Box<dyn Decoder>,
        mut format: Box<dyn FormatReader>,
        vec_buffer: Arc<Mutex<VecDeque<u8>>>,
        bits_per_sample: BitsPerSample,
    ) {
        thread::spawn(move || {
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
                                let mut sample_buffer = RawSampleBuffer::<i32>::new(duration, spec);
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
        thread::sleep(std::time::Duration::from_secs(1));
    }

    /// Plays a FLAC file
    /// - params:
    ///    - file: path to the FLAC file
    pub fn play(&mut self, file: String) -> Result<(), String> {
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
        self.fill_buffer(decoder, format, vec_buffer.clone(), BitsPerSample::from(bits_per_sample));

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

        let device = self.host.create_device(self.device_id)?;

        let streamparams = StreamParams {
            samplerate: samplerate.into(),
            channels,
            bits_per_sample: bits_per_sample.into(),
            buffer_length: 0,
            exclusive: true,
        };

        println!("Playing file path: {}", file);
        self.current_stream = Some(device.build_stream(streamparams, callback)?);
        //let mut current_stream = device.build_stream(streamparams, callback)?;
        match self.current_stream {
            Some(ref mut stream) => Ok(stream.start()?),
            None => Ok(())
        }
    }

    pub(crate) fn stop(&self) -> Result<(), String> {
        match self.current_stream {
            Some(ref stream) => stream.stop(),
            None => Ok(())
        }
    }
}
