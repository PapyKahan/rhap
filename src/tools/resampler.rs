use anyhow::Result;
use libsoxr::{Datatype, IOSpec, QualityFlags, QualityRecipe, QualitySpec, RuntimeSpec, Soxr};

use crate::{audio::BitsPerSample, player::StreamBuffer};

struct InternalSoxrResampler(pub Soxr);
impl InternalSoxrResampler {
    pub fn create(
        input_rate: f64,
        output_rate: f64,
        num_channels: u32,
        io_spec: Option<&IOSpec>,
        quality_spec: Option<&QualitySpec>,
        runtime_spec: Option<&RuntimeSpec>,
    ) -> Result<Self> {
        let soxr = Soxr::create(
            input_rate,
            output_rate,
            num_channels,
            io_spec,
            quality_spec,
            runtime_spec,
        )?;
        Ok(Self(soxr))
    }

    pub fn process<I, O>(&self, input: Option<&[I]>, output: &mut [O]) -> Result<()> {
        self.0.process(input, output)?;
        Ok(())
    }
}

// sync and send for MySoxr
unsafe impl Send for InternalSoxrResampler {}
unsafe impl Sync for InternalSoxrResampler {}

pub struct SoxrResampler<O>
{
    resampler: InternalSoxrResampler,
    output: Vec<O>,
}

impl<O> SoxrResampler<O>
where
    O: Default + Clone,
{
    pub fn new(
        from_samplerate: usize,
        to_samplerate: usize,
        from_bits_per_sample: BitsPerSample,
        to_bits_per_sample: BitsPerSample,
        duration: u64,
        num_channels: usize,
    ) -> Result<Self> {
        let duration = duration as usize;
        let mut output = Vec::<O>::with_capacity(duration * num_channels);
        output.resize(duration * num_channels, O::default());

        let input_type = match from_bits_per_sample {
            BitsPerSample::Bits16 => Datatype::Int16I,
            BitsPerSample::Bits24 => Datatype::Int32I,
            BitsPerSample::Bits32 => Datatype::Float32I,
        };
        let output_type = match to_bits_per_sample {
            BitsPerSample::Bits16 => Datatype::Int16I,
            BitsPerSample::Bits24 => Datatype::Int32I,
            BitsPerSample::Bits32 => Datatype::Float32I,
        };
        let io_spec = IOSpec::new(input_type, output_type);
        let runtime_spec = RuntimeSpec::new(num_channels as u32);
        let quality_spec = QualitySpec::new(&QualityRecipe::High, QualityFlags::ROLLOFF_NONE);
        let resampler = InternalSoxrResampler::create(
            from_samplerate as f64,
            to_samplerate as f64,
            num_channels as u32,
            Some(&io_spec),
            Some(&quality_spec),
            Some(&runtime_spec),
        )?;

        Ok(Self { resampler, output })
    }

    pub fn resample(&mut self, input: &StreamBuffer) -> Option<&[O]> {
        match input {
            StreamBuffer::I16(buffer) => {
                self.resampler
                    .process(Some(buffer.samples()), &mut self.output)
                    .unwrap();
                Some(&self.output)
            }
            StreamBuffer::I24(buffer) => {
                self.resampler
                    .process(Some(buffer.samples()), &mut self.output)
                    .unwrap();
                Some(&self.output)
            }
            StreamBuffer::F32(buffer) => {
                self.resampler
                    .process(Some(buffer.samples()), &mut self.output)
                    .unwrap();
                Some(&self.output)
            }
        }
    }
}
