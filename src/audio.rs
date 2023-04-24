pub mod api;

#[repr(u32)]
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum SampleRate {
    Rate44100Hz(u32) = 44100,
    Rate48000Hz(u32) = 48000,
    Rate88200Hz(u32) = 88200,
    Rate96000Hz(u32) = 96000,
    Rate176400Hz(u32) = 176400,
    Rate192000Hz(u32) = 192000,
}

impl SampleRate {
    pub fn value(&self) -> u32 {
        match self {
            SampleRate::Rate44100Hz(rate) => *rate,
            SampleRate::Rate48000Hz(rate) => *rate,
            SampleRate::Rate88200Hz(rate) => *rate,
            SampleRate::Rate96000Hz(rate) => *rate,
            SampleRate::Rate176400Hz(rate) => *rate,
            SampleRate::Rate192000Hz(rate) => *rate,
        }
    }

    pub fn from(value: u32) -> Option<SampleRate> {
        match value {
            44100 => Some(SampleRate::Rate44100Hz(44100)),
            48000 => Some(SampleRate::Rate48000Hz(48000)),
            88200 => Some(SampleRate::Rate88200Hz(88200)),
            96000 => Some(SampleRate::Rate96000Hz(96000)),
            176400 => Some(SampleRate::Rate176400Hz(176400)),
            192000 => Some(SampleRate::Rate192000Hz(192000)),
            _ => None,
        }
    }
}

#[repr(u8)]
#[derive(Debug, PartialEq, Eq, Clone)]
pub enum BitsPerSample {
    Bits8(u8) = 8,
    Bits16(u8) = 16,
    Bits24(u8) = 24,
    Bits32(u8) = 32,
}

impl BitsPerSample {
    pub fn value(&self) -> u8 {
        match self {
            BitsPerSample::Bits8(bits) => *bits,
            BitsPerSample::Bits16(bits) => *bits,
            BitsPerSample::Bits24(bits) => *bits,
            BitsPerSample::Bits32(bits) => *bits,
        }
    }

    pub fn from(value: u8) -> Option<BitsPerSample> {
        match value {
            8 => Some(BitsPerSample::Bits8(8)),
            16 => Some(BitsPerSample::Bits16(16)),
            24 => Some(BitsPerSample::Bits24(24)),
            32 => Some(BitsPerSample::Bits32(32)),
            _ => None,
        }
    }
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub struct StreamParams {
    pub device: Device,
    pub channels: u8,
    pub samplerate: SampleRate,
    pub bits_per_sample: BitsPerSample,
    pub exclusive: bool,
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub struct Device {
    pub id: u16,
    pub name: String,
    //capabilities: Capabilities,
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub struct Capabilities {
    max_sample_rate: SampleRate,
    min_sample_rate: SampleRate,
    max_bits_per_sample: BitsPerSample,
    min_bits_per_sample: BitsPerSample,
    max_channel_count: u8,
    min_channel_count: u8,
}

pub struct Stream<T: StreamTrait> {
    params: StreamParams,
    inner_stream: T,
}

pub enum DataProcessing {
    Continue,
    Complete,
    Abort,
}

pub trait StreamTrait {
    fn new<T>(params: StreamParams, callback : T) -> Result<Self, String>
    where
        Self: Sized,
        T: FnMut(&mut [u8], usize) -> Result<DataProcessing, String> + Send + 'static;
    fn start(&mut self) -> Result<(), String>;
    fn stop(&self) -> Result<(), String>;
    fn pause(&self) -> Result<(), String>;
    fn resume(&self) -> Result<(), String>;
    fn get_stream_params(&self) -> &StreamParams;
    fn set_stream_params(&mut self, stream_paramters: StreamParams);
}
