pub mod api;

#[repr(u32)]
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum SampleRate {
    Rate44100Hz = 44100,
    Rate48000Hz = 48000,
    Rate88200Hz = 88200,
    Rate96000Hz = 96000,
    Rate176400Hz = 176400,
    Rate192000Hz = 192000,
}

impl From<u32> for SampleRate {
    fn from(value: u32) -> Self {
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

#[repr(u8)]
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum BitsPerSample {
    Bits8 = 8,
    Bits16 = 16,
    Bits24 = 24,
    Bits32 = 32,
}

impl From<u8> for BitsPerSample {
    fn from(value: u8) -> Self {
        match value {
            8 => BitsPerSample::Bits8,
            16 => BitsPerSample::Bits16,
            24 => BitsPerSample::Bits24,
            32 => BitsPerSample::Bits32,
            _ => panic!("Invalid bits per sample"),
        }
    }
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub struct StreamParams {
    pub device: Device,
    pub channels: u8,
    pub samplerate: SampleRate,
    pub bits_per_sample: BitsPerSample,
    pub buffer_length: i64,
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

pub enum StreamFlow {
    Continue,
    Complete,
    Abort,
}

pub trait StreamTrait {
    fn new<T>(params: StreamParams, callback : T) -> Result<Self, String>
    where
        Self: Sized,
        T: FnMut(&mut [u8], usize) -> Result<StreamFlow, String> + Send + 'static;
    fn start(&mut self) -> Result<(), String>;
    fn stop(&self) -> Result<(), String>;
    fn pause(&self) -> Result<(), String>;
    fn resume(&self) -> Result<(), String>;
    fn get_stream_params(&self) -> &StreamParams;
    fn set_stream_params(&mut self, stream_paramters: StreamParams);
}
