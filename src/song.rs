use std::sync::{Arc, Mutex};

use anyhow::Result;
use symphonia::core::{codecs::{DecoderOptions, Decoder}, io::MediaSourceStream, probe::Hint, formats::FormatReader, meta::{MetadataRevision, StandardTagKey}};

use crate::audio::{SampleRate, BitsPerSample};

pub struct Song {
    pub format : Arc<Mutex<Box<dyn FormatReader>>>,
    pub decoder: Arc<Mutex<Box<dyn Decoder>>>,
    pub sample: SampleRate,
    pub channels: usize,
    pub bits_per_sample: BitsPerSample,
    pub title: String,
    pub artist: String,
    //pub duration: Duration,
}

impl Song {
    pub fn new(path: String) -> Result<Self> {
        let source = std::fs::File::open(path.clone())?;
        let mss = MediaSourceStream::new(Box::new(source), Default::default());
        let hint = Hint::new();
        let meta_opts = Default::default();
        let fmt_opts = Default::default();
        let probed = symphonia::default::get_probe()
            .format(&hint, mss, &fmt_opts, &meta_opts)
            .expect("unsupported format");

        let mut format = probed.format;
        let track = format.tracks().get(0).unwrap().clone();
        let samplerate = track.codec_params.sample_rate.unwrap();
        let channels = track.codec_params.channels.unwrap().count();
        let bits_per_sample = track.codec_params.bits_per_sample.unwrap_or(16) as u8;

        let metadata = match format.metadata().current() {
            Some(metadata) => metadata.clone(),
            None => MetadataRevision::default().clone(),
        };

        let artist = metadata.tags().iter().find(|e| e.std_key == Some(StandardTagKey::Artist)).unwrap().value.to_string();
        let title = metadata.tags().iter().find(|e| e.std_key == Some(StandardTagKey::TrackTitle)).unwrap().value.to_string();

        // Create a decoder for the track.
        let decoder = symphonia::default::get_codecs()
            .make(&track.codec_params, &DecoderOptions { verify: true })?;
        
        Ok(Self {
            format: Arc::new(Mutex::new(format)),
            decoder: Arc::new(Mutex::new(decoder)),
            sample: SampleRate::from(samplerate),
            channels,
            bits_per_sample: BitsPerSample::from(bits_per_sample),
            title,
            artist
        })
    }
}
