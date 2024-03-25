use anyhow::Result;
use std::sync::Arc;
use symphonia::core::{
    audio::Layout,
    codecs::{Decoder, DecoderOptions},
    formats::{FormatReader, Track},
    io::MediaSourceStream,
    meta::{MetadataRevision, StandardTagKey},
    probe::Hint,
};
use tokio::sync::Mutex;

use crate::audio::{BitsPerSample, SampleRate};

pub struct Song {
    pub format: Arc<Mutex<Box<dyn FormatReader>>>,
    pub decoder: Arc<Mutex<Box<dyn Decoder>>>,
    pub sample: SampleRate,
    pub channels: usize,
    pub bits_per_sample: BitsPerSample,
    pub title: String,
    pub artist: String,
    pub duration: u64,
    track: Track,
}

impl Song {
    pub fn new(path: String) -> Result<Self> {
        let source = std::fs::File::open(path.clone())?;
        let mss = MediaSourceStream::new(Box::new(source), Default::default());
        let hint = Hint::new();
        let meta_opts = Default::default();
        let fmt_opts = Default::default();
        let probed = symphonia::default::get_probe().format(&hint, mss, &fmt_opts, &meta_opts)?;

        let mut format = probed.format;
        let track = format.tracks().get(0).unwrap().clone();
        let samplerate = track.codec_params.sample_rate.unwrap_or(44100);
        let channels = track
            .codec_params
            .channels
            .unwrap_or(Layout::Stereo.into_channels())
            .count();
        let bits_per_sample = track.codec_params.bits_per_sample.unwrap_or(16) as u8;

        let metadata = match format.metadata().skip_to_latest() {
            Some(metadata) => metadata.clone(),
            None => MetadataRevision::default().clone(),
        };

        let artist = metadata
            .tags()
            .iter()
            .find(|e| e.std_key == Some(StandardTagKey::Artist))
            .unwrap()
            .value
            .to_string();
        let title = metadata
            .tags()
            .iter()
            .find(|e| e.std_key == Some(StandardTagKey::TrackTitle))
            .unwrap()
            .value
            .to_string();
        let duration = track
            .codec_params.time_base.unwrap_or(Default::default()).calc_time(track.codec_params.n_frames.unwrap_or(0)).seconds;

        // Create a decoder for the track.
        let decoder = symphonia::default::get_codecs()
            .make(&track.codec_params, &DecoderOptions { verify: true })?;

        Ok(Self {
            format: Arc::new(Mutex::new(format)),
            decoder: Arc::new(Mutex::new(decoder)),
            sample: SampleRate::from(samplerate as usize),
            channels,
            bits_per_sample: BitsPerSample::from(bits_per_sample as usize),
            title,
            artist,
            duration,
            track,
        })
    }

    pub fn info(&self) -> String {
        format!(
            "{}bits - {}KHz",
            self.bits_per_sample as usize,
            (self.sample as usize) as f32 / 1000.0
        )
    }

    pub fn formated_duration(&self) -> String {
        if let Some(tb) = self.track.codec_params.time_base {
            let d = tb.calc_time(self.duration);
            let hours = d.seconds / (60 * 60);
            let mins = (d.seconds % (60 * 60)) / 60;
            let secs = (d.seconds % 60) + d.frac as u64;
            match hours {
                0 => match mins {
                    0 => {
                        format!("00:{:0>2}", secs)
                    }
                    _ => {
                        format!("{:0>2}:{:0>2}", mins, secs)
                    }
                },
                _ => {
                    format!("{}:{:0>2}:{:0>2}", hours, mins, secs)
                }
            }
        } else {
            String::default()
        }
    }
}
