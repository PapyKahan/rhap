use claxon::{Block, FlacReader};
use std::collections::VecDeque;
use std::fs::File;
use std::sync::{Arc, Mutex};
use std::thread;

use crate::audio::{
    StreamFlow, Device, StreamParams, StreamTrait
};
use crate::audio::api::wasapi::stream::Stream;

pub struct Player {
    device_id: u16,
}

impl Player {
    pub fn new(device_id : u16) -> Self {
        Player {device_id}
    }

    #[inline(always)]
    fn fill_buffer(&self, mut flac_reader: FlacReader<File>, vec_buffer: Arc<Mutex<VecDeque<u8>>>, bytes: u8) {
        thread::spawn(move || {
            let mut frame_reader = flac_reader.blocks();
            let mut block = Block::empty();
            loop {
                match frame_reader.read_next_or_eof(block.into_buffer()) {
                    Ok(Some(next_block)) => {
                        block = next_block;
                    }
                    Ok(None) => break, // EOF.
                    Err(error) => panic!("{}", error),
                };
    
                for samples in block.stereo_samples() {
                    let left = samples.0.to_le_bytes();
                    let mut copied_bytes = 0;
                    for l in left.iter() {
                        vec_buffer.lock().unwrap().push_back(*l);
                        copied_bytes += 1;
                        if copied_bytes >= bytes {
                            break;
                        }
                    }
    
                    let right = samples.1.to_le_bytes();
                    copied_bytes = 0;
                    for r in right.iter() {
                        vec_buffer.lock().unwrap().push_back(*r);
                        copied_bytes += 1;
                        if copied_bytes >= bytes {
                            break;
                        }
                    }
                }
            }
        });
        thread::sleep(std::time::Duration::from_secs(1));
    }

    
    /// Plays a FLAC file
    /// - params:
    ///    - file: path to the FLAC file
    pub fn play(&self, file: String) -> Result<(), String> {
        let flac_reader = FlacReader::open(&file).expect("Failed to open FLAC file");
        let samplerate = flac_reader.streaminfo().sample_rate;
        let channels = flac_reader.streaminfo().channels as u8;
        let bits_per_sample = flac_reader.streaminfo().bits_per_sample as u8;
        let bytes = bits_per_sample / 8;

        let vec_buffer = Arc::new(Mutex::new(VecDeque::new()));
        self.fill_buffer(flac_reader, vec_buffer.clone(), bytes);

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
                buffer_length: 0,
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
            Ok(_) => {},
            Err(e) => {
                return Err(format!("Failed to start stream: {}", e))
            }
        };

        match stream.stop() {
            Ok(_) => Ok(()),
            Err(e) => {
                return Err(format!("Failed to stop stream: {}", e));
            }
        }
    }
}
