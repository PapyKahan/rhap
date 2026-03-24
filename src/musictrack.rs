use anyhow::Result;
use std::sync::Arc;
use symphonia::core::{
    audio::Layout,
    codecs::{Decoder, DecoderOptions},
    formats::FormatReader,
    io::MediaSourceStream,
    meta::{StandardTagKey, StandardVisualKey},
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
    pub album: String,
    pub duration: Time,
    pub probed: bool,
    pub cover_art: Option<Arc<[u8]>>,
    pub cover_art_mime: Option<String>,
}

impl MusicTrack {
    /// Create an unprobed entry using only the filename — no I/O.
    pub fn from_path(path: String) -> Self {
        let title = std::path::Path::new(&path)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("Unknown")
            .to_string();
        Self {
            path,
            sample: SampleRate(0),
            channels: 0,
            bits_per_sample: BitsPerSample(0),
            title,
            artist: String::new(),
            album: String::new(),
            duration: Time::default(),
            probed: false,
            cover_art: None,
            cover_art_mime: None,
        }
    }

    /// Scan metadata only — does not create a decoder or retain the file handle.
    /// Used by the background prober for playlist display.
    pub fn new(path: String) -> Result<Self> {
        let source = std::fs::File::open(&path)?;
        let mss = MediaSourceStream::new(Box::new(source), Default::default());
        let mut hint = Hint::new();
        if let Some(ext) = std::path::Path::new(&path).extension().and_then(|e| e.to_str()) {
            hint.with_extension(ext);
        }
        let probed = symphonia::default::get_probe()
            .format(&hint, mss, &Default::default(), &Default::default())?;

        let mut format = probed.format;
        let track = format
            .tracks()
            .first()
            .ok_or_else(|| anyhow::anyhow!("No tracks found in {}", path))?
            .clone();
        let samplerate = track.codec_params.sample_rate.unwrap_or(44100);
        let channels = track
            .codec_params
            .channels
            .unwrap_or(Layout::Stereo.into_channels())
            .count();
        let bits_per_sample = track.codec_params.bits_per_sample.unwrap_or(16) as u16;

        let metadata = format
            .metadata()
            .skip_to_latest()
            .cloned()
            .unwrap_or_default();

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
        let album = metadata
            .tags()
            .iter()
            .find(|e| e.std_key == Some(StandardTagKey::Album))
            .map(|t| t.value.to_string())
            .unwrap_or_else(|| "Unknown Album".to_string());
        let duration = track
            .codec_params
            .time_base
            .unwrap_or_default()
            .calc_time(track.codec_params.n_frames.unwrap_or(0));

        let visuals = metadata.visuals();
        let visual = visuals
            .iter()
            .find(|v| v.usage == Some(StandardVisualKey::FrontCover))
            .or_else(|| visuals.first());
        let cover_art = visual.map(|v| Arc::from(v.data.as_ref()));
        let cover_art_mime = visual.map(|v| v.media_type.clone());

        Ok(Self {
            path,
            sample: SampleRate(samplerate),
            channels,
            bits_per_sample: BitsPerSample(bits_per_sample),
            title,
            artist,
            album,
            duration,
            probed: true,
            cover_art,
            cover_art_mime,
        })
    }

    /// Probe metadata AND prepare for playback in a single file open.
    /// Returns the probed track and a ready-to-use PlaybackHandle.
    /// Use this when the track hasn't been probed yet to avoid opening the file twice.
    pub fn probe_and_open(path: String) -> Result<(Self, PlaybackHandle)> {
        let source = std::fs::File::open(&path)?;
        let mss = MediaSourceStream::new(Box::new(source), Default::default());
        let mut hint = Hint::new();
        if let Some(ext) = std::path::Path::new(&path).extension().and_then(|e| e.to_str()) {
            hint.with_extension(ext);
        }
        let probed = symphonia::default::get_probe()
            .format(&hint, mss, &Default::default(), &Default::default())?;

        let mut format = probed.format;
        let track = format
            .tracks()
            .first()
            .ok_or_else(|| anyhow::anyhow!("No tracks found in {}", path))?
            .clone();

        let samplerate = track.codec_params.sample_rate.unwrap_or(44100);
        let channels = track
            .codec_params
            .channels
            .unwrap_or(Layout::Stereo.into_channels())
            .count();
        let bits_per_sample = track.codec_params.bits_per_sample.unwrap_or(16) as u16;

        let metadata = format
            .metadata()
            .skip_to_latest()
            .cloned()
            .unwrap_or_default();

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
        let album = metadata
            .tags()
            .iter()
            .find(|e| e.std_key == Some(StandardTagKey::Album))
            .map(|t| t.value.to_string())
            .unwrap_or_else(|| "Unknown Album".to_string());
        let duration = track
            .codec_params
            .time_base
            .unwrap_or_default()
            .calc_time(track.codec_params.n_frames.unwrap_or(0));

        let visuals = metadata.visuals();
        let visual = visuals
            .iter()
            .find(|v| v.usage == Some(StandardVisualKey::FrontCover))
            .or_else(|| visuals.first());
        let cover_art = visual.map(|v| Arc::from(v.data.as_ref()));
        let cover_art_mime = visual.map(|v| v.media_type.clone());

        let decoder = symphonia::default::get_codecs()
            .make(&track.codec_params, &DecoderOptions { verify: false })?;

        let music_track = Self {
            path,
            sample: SampleRate(samplerate),
            channels,
            bits_per_sample: BitsPerSample(bits_per_sample),
            title,
            artist,
            album,
            duration,
            probed: true,
            cover_art,
            cover_art_mime,
        };

        let handle = PlaybackHandle { format, decoder };
        Ok((music_track, handle))
    }

    /// Open the file for playback — creates the FormatReader and Decoder on demand.
    /// Use this when the track has already been probed (metadata is cached).
    pub fn open_for_playback(&self) -> Result<PlaybackHandle> {
        let source = std::fs::File::open(&self.path)?;
        let mss = MediaSourceStream::new(Box::new(source), Default::default());
        let mut hint = Hint::new();
        if let Some(ext) = std::path::Path::new(&self.path).extension().and_then(|e| e.to_str()) {
            hint.with_extension(ext);
        }
        let probed = symphonia::default::get_probe()
            .format(&hint, mss, &Default::default(), &Default::default())?;

        let format = probed.format;
        let track = format
            .tracks()
            .first()
            .ok_or_else(|| anyhow::anyhow!("No tracks found in {}", self.path))?
            .clone();
        let decoder = symphonia::default::get_codecs()
            .make(&track.codec_params, &DecoderOptions { verify: false })?;

        Ok(PlaybackHandle { format, decoder })
    }

    pub fn info(&self) -> String {
        format!("{} - {}", self.bits_per_sample, self.sample)
    }

}

pub struct PlaybackHandle {
    pub format: Box<dyn FormatReader>,
    pub decoder: Box<dyn Decoder>,
}
