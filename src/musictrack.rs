use anyhow::Result;
use symphonia::core::{
    audio::Layout,
    codecs::{Decoder, DecoderOptions},
    formats::FormatReader,
    io::MediaSourceStream,
    meta::{MetadataRevision, StandardTagKey},
    probe::Hint,
    units::Time,
};

use crate::audio::{BitsPerSample, SampleRate};

pub struct MusicTrack {
    pub path: String,
    pub sample: SampleRate,
    pub channels: usize,
    pub bits_per_sample: BitsPerSample,
    pub title: String,
    pub artist: String,
    pub duration: Time,
}

impl MusicTrack {
    /// Scan metadata only — does not create a decoder or retain the file handle.
    pub fn new(path: String) -> Result<Self> {
        let source = std::fs::File::open(path.clone())?;
        let mss = MediaSourceStream::new(Box::new(source), Default::default());
        let mut hint = Hint::new();
        if let Some(ext) = std::path::Path::new(&path).extension().and_then(|e| e.to_str()) {
            hint.with_extension(ext);
        }
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
            .map(|t| t.value.to_string())
            .unwrap_or_else(|| "Unknown Artist".to_string());
        let title = metadata
            .tags()
            .iter()
            .find(|e| e.std_key == Some(StandardTagKey::TrackTitle))
            .map(|t| t.value.to_string())
            .unwrap_or_else(|| "Unknown Title".to_string());
        let duration = track
            .codec_params
            .time_base
            .unwrap_or(Default::default())
            .calc_time(track.codec_params.n_frames.unwrap_or(0));

        Ok(Self {
            path,
            sample: SampleRate::from(samplerate as usize),
            channels,
            bits_per_sample: BitsPerSample::from(bits_per_sample as usize),
            title,
            artist,
            duration,
        })
    }

    /// Open the file for playback — creates the FormatReader and Decoder on demand.
    pub fn open_for_playback(&self) -> Result<PlaybackHandle> {
        let source = std::fs::File::open(self.path.clone())?;
        let mss = MediaSourceStream::new(Box::new(source), Default::default());
        let mut hint = Hint::new();
        if let Some(ext) = std::path::Path::new(&self.path).extension().and_then(|e| e.to_str()) {
            hint.with_extension(ext);
        }
        let meta_opts = Default::default();
        let fmt_opts = Default::default();
        let probed = symphonia::default::get_probe().format(&hint, mss, &fmt_opts, &meta_opts)?;

        let format = probed.format;
        let track = format.tracks().get(0).unwrap().clone();
        let decoder = symphonia::default::get_codecs()
            .make(&track.codec_params, &DecoderOptions { verify: false })?;

        Ok(PlaybackHandle {
            format,
            decoder,
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
        let hours = self.duration.seconds / (60 * 60);
        let mins = (self.duration.seconds % (60 * 60)) / 60;
        let secs = (self.duration.seconds % 60) + self.duration.frac as u64;
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
    }
}

pub struct PlaybackHandle {
    pub format: Box<dyn FormatReader>,
    pub decoder: Box<dyn Decoder>,
}
