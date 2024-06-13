use anyhow::Result;
use libsoxr::{Datatype, IOSpec, QualityFlags, QualityRecipe, QualitySpec, RuntimeSpec, Soxr};
use rubato::{
    calculate_cutoff, Resampler, Sample, SincInterpolationParameters, SincInterpolationType, WindowFunction
};

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

pub struct SoxrResampler<O> {
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
        let quality_spec = QualitySpec::new(&QualityRecipe::Quick, QualityFlags::VR);
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
                self.output.fill(O::default());
                self.resampler
                    .process(Some(buffer.samples()), &mut self.output)
                    .unwrap();
                Some(&self.output)
            }
            StreamBuffer::I24(buffer) => {
                self.output.fill(O::default());
                self.resampler
                    .process(Some(buffer.samples()), &mut self.output)
                    .unwrap();
                Some(&self.output)
            }
            StreamBuffer::F32(buffer) => {
                self.output.fill(O::default());
                self.resampler
                    .process(Some(buffer.samples()), &mut self.output)
                    .unwrap();
                Some(&self.output)
            }
        }
    }
}

struct RubatoResampler<O> {
    resampler: rubato::SincFixedIn<f32>,
    output: Vec<O>,
}

impl<O> RubatoResampler<O>
where
    O: Default + Clone,
{
    pub fn new(
        from_samplerate: usize,
        to_samplerate: usize,
        num_channels: usize,
        duration: u64,
    ) -> Self {
        let duration = duration as usize;
        let mut output = Vec::<O>::with_capacity(duration * num_channels);
        output.resize(duration * num_channels, O::default());

        let ratio = to_samplerate as f32 / from_samplerate as f32;
        let sinc_len = 256;
        let oversampling_factor = sinc_len;
        let interpolation = SincInterpolationType::Nearest;
        let window = WindowFunction::BlackmanHarris2;

        let f_cutoff = calculate_cutoff(sinc_len, window);
        let params = SincInterpolationParameters {
            sinc_len,
            f_cutoff,
            interpolation,
            oversampling_factor,
            window,
        };

        let resampler =
            rubato::SincFixedIn::<f32>::new(ratio as f64, 1.0, params, duration, num_channels)
                .unwrap();
        let output_buffer = resampler.output_buffer_allocate(true);

        Self { resampler, output }
    }

    pub fn resample(&mut self, input: &StreamBuffer) -> Option<&[O]> {
        match input {
            StreamBuffer::I16(buffer) => {
                self.output.fill(O::default());
                self.resampler
                    .process(buffer.samples(), &mut self.output)
                    .unwrap();
                Some(&self.output)
            }
            StreamBuffer::I24(buffer) => {
                self.output.fill(O::default());
                self.resampler
                    .process(buffer.samples(), &mut self.output)
                    .unwrap();
                Some(&self.output)
            }
            StreamBuffer::F32(buffer) => {
                self.output.fill(O::default());
                self.resampler
                    .process(buffer.samples(), &mut self.output)
                    .unwrap();
                Some(&self.output)
            }
        }
    }
}
