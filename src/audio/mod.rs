pub(crate) mod api;
pub(crate) mod host;
pub(crate) mod device;

pub use host::{HostTrait, Host};
pub use device::{DeviceTrait, Device};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SampleRate(pub u32);

impl std::fmt::Display for SampleRate {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.0 % 1000 == 0 {
            write!(f, "{}kHz", self.0 / 1000)
        } else {
            write!(f, "{:.1}kHz", self.0 as f64 / 1000.0)
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct BitsPerSample(pub u16);

impl std::fmt::Display for BitsPerSample {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}bit", self.0)
    }
}

#[derive(Clone)]
pub struct Capabilities {
    pub sample_rates: Vec<SampleRate>,
    pub bits_per_samples: Vec<BitsPerSample>,
}

impl Capabilities {
    pub fn all_possible() -> Self {
        Self {
            sample_rates: vec![
                SampleRate(8000),
                SampleRate(11025),
                SampleRate(16000),
                SampleRate(22050),
                SampleRate(44100),
                SampleRate(48000),
                SampleRate(88200),
                SampleRate(96000),
                SampleRate(176400),
                SampleRate(192000),
                SampleRate(352800),
                SampleRate(384000),
                SampleRate(768000),
            ],
            bits_per_samples: vec![
                BitsPerSample(16),
                BitsPerSample(24),
                BitsPerSample(32),
            ],
        }
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct StreamParams {
    pub channels: u8,
    pub samplerate: SampleRate,
    pub bits_per_sample: BitsPerSample,
    pub exclusive: bool,
    pub pollmode: bool,
}

impl StreamParams {
    pub fn adjust_with_capabilities(&self, capabilities: &Capabilities) -> StreamParams {
        let contains_sample_rates = capabilities.sample_rates.contains(&self.samplerate);
        let contains_bits_per_samples = capabilities.bits_per_samples.contains(&self.bits_per_sample);
        if !contains_sample_rates || !contains_bits_per_samples {
            let samplerate = if contains_sample_rates {
                self.samplerate
            } else {
                capabilities.sample_rates.last().copied().unwrap_or(self.samplerate)
            };
            let bits_per_sample = if contains_bits_per_samples {
                self.bits_per_sample
            } else {
                capabilities.bits_per_samples.last().copied().unwrap_or(self.bits_per_sample)
            };
            StreamParams {
                samplerate,
                bits_per_sample,
                ..*self
            }
        } else {
            *self
        }
    }
}
