pub(crate) mod api;
pub(crate) mod host;
pub(crate) mod device;

pub use host::{HostTrait, Host};
pub use device::{DeviceTrait, Device};

#[repr(usize)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SampleRate {
    Rate44100Hz = 44100,
    Rate48000Hz = 48000,
    Rate88200Hz = 88200,
    Rate96000Hz = 96000,
    Rate176400Hz = 176400,
    Rate192000Hz = 192000,
}

impl From<usize> for SampleRate {
    fn from(value: usize) -> Self {
        match value {
            44100 => SampleRate::Rate44100Hz,
            48000 => SampleRate::Rate48000Hz,
            88200 => SampleRate::Rate88200Hz,
            96000 => SampleRate::Rate96000Hz,
            176400 => SampleRate::Rate176400Hz,
            192000 => SampleRate::Rate192000Hz,
            _ => panic!("Invalid sample rate"),
        }
    }
}

#[repr(usize)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum BitsPerSample {
    Bits8 = 8,
    Bits16 = 16,
    Bits24 = 24,
    Bits32 = 32,
}

impl From<usize> for BitsPerSample {
    fn from(value: usize) -> Self {
        match value {
            8 => BitsPerSample::Bits8,
            16 => BitsPerSample::Bits16,
            24 => BitsPerSample::Bits24,
            32 => BitsPerSample::Bits32,
            _ => panic!("Invalid bits per sample"),
        }
    }
}

pub struct Capabilities {
    pub sample_rates: Vec<SampleRate>,
    pub bits_per_samples: Vec<BitsPerSample>,
}

impl Capabilities {
    pub fn default() -> Self {
        Self {
            sample_rates: vec![
                SampleRate::Rate44100Hz,
                SampleRate::Rate48000Hz,
                SampleRate::Rate88200Hz,
                SampleRate::Rate96000Hz,
                SampleRate::Rate176400Hz,
                SampleRate::Rate192000Hz,
            ],
            bits_per_samples: vec![
                BitsPerSample::Bits16,
                BitsPerSample::Bits24,
                BitsPerSample::Bits32,
            ],
        }
    }
}

#[derive(Debug, Copy, Clone)]
pub struct StreamParams {
    pub channels: u8,
    pub samplerate: SampleRate,
    pub bits_per_sample: BitsPerSample,
    pub exclusive: bool,
}

#[derive(Copy, Clone)]
pub enum StreamingCommand {
    Pause,
    Resume,
}

#[derive(Copy, Clone)]
pub enum StreamingData {
    Data(u8),
    EndOfStream
}

