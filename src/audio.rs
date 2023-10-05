pub mod api;

#[repr(u32)]
#[derive(Debug, Clone, Copy)]
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
#[derive(Debug, Clone, Copy)]
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

#[derive(Debug, Clone)]
pub struct StreamParams {
    pub channels: u8,
    pub samplerate: SampleRate,
    pub bits_per_sample: BitsPerSample,
    pub buffer_length: i64,
    pub exclusive: bool,
}

#[derive(Clone)]
pub enum StreamFlow {
    Stop,
    Pause,
    Continue,
    Complete,
}

pub trait StreamTrait : Send + Sync {
    fn start(&mut self, callback : &mut dyn FnMut(&mut [u8], usize) -> Result<StreamFlow, Box<dyn std::error::Error>>) -> Result<(), Box<dyn std::error::Error>>;
    fn stop(&mut self) -> Result<(), Box<dyn std::error::Error>>;
    fn pause(&mut self) -> Result<(), Box<dyn std::error::Error>>;
    fn resume(&mut self) -> Result<(), Box<dyn std::error::Error>>;
    fn get_stream_params(&self) -> &StreamParams;
    fn set_stream_params(&mut self, stream_paramters: StreamParams);
}

pub enum Stream {
    Wasapi(api::wasapi::stream::Stream)
}

impl StreamTrait for Stream {
    fn start(&mut self, callback : &mut dyn FnMut(&mut [u8], usize) -> Result<StreamFlow, Box<dyn std::error::Error>>) -> Result<(), Box<dyn std::error::Error>> {
        let stream = match self {
            Self::Wasapi(stream) => stream
        };
        stream.start(callback)
    }

    fn stop(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let stream = match self {
            Self::Wasapi(stream) => stream
        };
        stream.stop()
    }

    fn pause(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let stream = match self {
            Self::Wasapi(stream) => stream
        };
        stream.pause()
    }

    fn resume(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let stream = match self {
            Self::Wasapi(stream) => stream
        };
        stream.resume()
    }

    fn get_stream_params(&self) -> &StreamParams {
        let stream = match self {
            Self::Wasapi(stream) => stream
        };
        stream.get_stream_params()
    }

    fn set_stream_params(&mut self, stream_paramters: StreamParams) {
        let stream = match self {
            Self::Wasapi(stream) => stream
        };
        stream.set_stream_params(stream_paramters)
    }
}

pub trait DeviceTrait : Send + Sync  {
    fn is_default(&self) -> bool;
    fn name(&self) -> String;
    fn build_stream(&self, params: StreamParams) -> Result<Stream, Box<dyn std::error::Error>>;
}

pub enum Device {
    Wasapi(api::wasapi::device::Device)
}

impl DeviceTrait for Device {
    fn is_default(&self) -> bool {
        let device = match self {
            Self::Wasapi(device) => device
        };
        device.is_default()
    }

    fn name(&self) -> String {
        let device = match self {
            Self::Wasapi(device) => device
        };
        device.name()
    }

    fn build_stream(&self, params: StreamParams) -> Result<Stream, Box<dyn std::error::Error>> {
        let device = match self {
            Self::Wasapi(device) => device
        };

        device.build_stream(params)
    }
}

pub trait HostTrait: Send + Sync {
    fn create_device(&self, id: Option<u32>) -> Result<Device, Box<dyn std::error::Error>>;
    fn get_devices(&self) -> Result<Vec<Device>, Box<dyn std::error::Error>>;
}

#[derive(Clone)]
pub enum Host {
    Wasapi(api::wasapi::host::Host)
}

impl HostTrait for Host {
    fn get_devices(&self) -> Result<Vec<Device>, Box<dyn std::error::Error>> {
        match self {
            Self::Wasapi(host) => {
                host.get_devices()
            }
        }
    }

    fn create_device(&self, id: Option<u32>) -> Result<Device, Box<dyn std::error::Error>> {
        match self {
            Self::Wasapi(host) => {
                host.create_device(id)
            }
        }
    }
}

pub(crate) fn create_host(host_name : &str) -> Host {
    match host_name {
        "wasapi" => Host::Wasapi(api::wasapi::host::Host::new()),
        _ => Host::Wasapi(api::wasapi::host::Host::new())
    }
}
