//
// TODO add commandline parsing : https://docs.rs/clap/latest/clap/
// reference : Shared mode streaming : https://learn.microsoft.com/en-us/windows/win32/coreaudio/rendering-a-stream
// reference : Exclusive mode streaming : https://learn.microsoft.com/en-us/windows/win32/coreaudio/exclusive-mode-streams
// reference : https://www.hresult.info/FACILITY_AUDCLNT
//
use claxon::{Block, FlacReader};
use std::collections::VecDeque;
use std::fs::File;
use std::sync::{Arc, Mutex};
use std::thread;

mod audio;
use crate::audio::{
    api::wasapi::{enumerate_devices, stream::WasapiStream},
    BitsPerSample, DataProcessing, Device, SampleRate, Stream, StreamParams,
};

const DEVICE_ID: u16 = 2;

fn fill_buffer(
    mut flac_reader: FlacReader<File>,
    vec_buffer: Arc<Mutex<VecDeque<u8>>>,
    bytes: u8,
) {
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
    thread::sleep(std::time::Duration::from_millis(1000));
}

fn main() -> Result<(), ()> {
    let args = std::env::args().collect::<Vec<String>>();
    let file_path = match args.len() {
        2 => &args[1],
        _ => {
            println!("Usage: rhap <file>");
            let devices = enumerate_devices().unwrap();
            for dev in devices {
                println!("Device: id={}, name={}", dev.index, dev.name);
            }
            return Ok(());
        }
    };


    let flac_reader = FlacReader::open(&file_path).expect("Failed to open FLAC file");

    let sample_rate = SampleRate::from(flac_reader.streaminfo().sample_rate);
    let channels = flac_reader.streaminfo().channels as u8;
    let bits = flac_reader.streaminfo().bits_per_sample as u8;
    let bits_per_sample = BitsPerSample::from(bits);
    let bytes = bits / 8;

    let vec_buffer = Arc::new(Mutex::new(VecDeque::new()));
    fill_buffer(flac_reader, vec_buffer.clone(), bytes);

    let callback = move |data: &mut [u8], buffer_size: usize| -> Result<DataProcessing, String> {
        let mut data_processing = DataProcessing::Continue;
        for i in 0..buffer_size {
            if vec_buffer.lock().unwrap().is_empty() {
                data_processing = DataProcessing::Complete;
                break;
            }
            data[i] = vec_buffer.lock().unwrap().pop_front().unwrap();
        }
        Ok(data_processing)
    };

    let mut stream = match Stream::<WasapiStream>::new(
        StreamParams {
            device: Device {
                id: DEVICE_ID,
                name: String::from(""),
            },
            samplerate: sample_rate.unwrap(),
            channels,
            bits_per_sample: bits_per_sample.unwrap(),
            exclusive: true,
        },
        callback,
    ) {
        Ok(s) => s,
        Err(e) => {
            println!("Failed to create stream: {}", e);
            return Ok(());
        }
    };

    println!("Playing file path: {}", file_path);
    match stream.start() {
        Ok(_) => {}
        Err(e) => {
            println!("Failed to start stream: {}", e);
            return Ok(());
        }
    }

    match stream.stop() {
        Ok(_) => {}
        Err(e) => {
            println!("Failed to stop stream: {}", e);
            return Ok(());
        }
    }

    return Ok(());
}
