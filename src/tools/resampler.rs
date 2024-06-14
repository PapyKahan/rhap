use anyhow::Result;
use libsoxr::{Datatype, IOSpec, QualityFlags, QualityRecipe, QualitySpec, RuntimeSpec, Soxr};
use rubato::{
    calculate_cutoff, Resampler, SincInterpolationParameters, SincInterpolationType, WindowFunction,
};
use symphonia::core::{
    audio::{AudioBuffer, AudioBufferRef, Signal},
    conv::{FromSample, IntoSample},
    sample::Sample,
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
    duration: usize,
    channels: usize,
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

        Ok(Self {
            resampler,
            output,
            duration,
            channels: num_channels,
        })
    }

    pub fn resample(&mut self, input: &AudioBufferRef<'_>) -> Option<&[O]> {
        match input {
            AudioBufferRef::S16(buffer) => {
                self.output.fill(O::default());
                let mut inp = vec![0.0 as f32; self.duration * self.channels];
                for i in 0..self.channels {
                    let src = buffer.chan(i);
                    for (j, dst) in inp.iter_mut().enumerate() {
                        *dst = src[j].into_sample();
                    }
                }
                self.resampler
                    .process(Some(&inp), &mut self.output)
                    .unwrap();
                Some(&self.output)
            }
            AudioBufferRef::S24(buffer) => {
                self.output.fill(O::default());
                let mut inp = vec![0.0 as f32; self.duration * self.channels];
                for i in 0..self.channels {
                    let src = buffer.chan(i);
                    for (j, dst) in inp.iter_mut().enumerate() {
                        *dst = src[j].into_sample();
                    }
                }
                self.resampler
                    .process(Some(&inp), &mut self.output)
                    .unwrap();
                Some(&self.output)
            }
            AudioBufferRef::F32(buffer) => {
                self.output.fill(O::default());
                let mut inp = vec![0.0 as f32; self.duration * self.channels];
                for i in 0..self.channels {
                    let src = buffer.chan(i);
                    for (j, dst) in inp.iter_mut().enumerate() {
                        *dst = src[j].into_sample();
                    }
                }
                self.resampler
                    .process(Some(&inp), &mut self.output)
                    .unwrap();
                Some(&self.output)
            }
            _ => None,
        }
    }
}

pub struct RubatoResampler<O> {
    resampler: rubato::SincFixedIn<f32>,
    input: Vec<Vec<f32>>,
    output: Vec<Vec<f32>>,
    interleaved_output: Vec<O>,
    duration: usize,
    channels: usize,
    _from_bits_per_sample: BitsPerSample,
    _to_bits_per_sample: BitsPerSample,
}

impl<O> RubatoResampler<O>
where
    O: Sample + FromSample<f32> + IntoSample<O> + Default + Clone,
{
    pub fn new(
        from_samplerate: usize,
        to_samplerate: usize,
        _from_bits_per_sample: BitsPerSample,
        _to_bits_per_sample: BitsPerSample,
        duration: u64,
        num_channels: usize,
    ) -> Result<Self> {
        let duration = duration as usize;
        let mut interleaved_output = Vec::<O>::with_capacity(duration * num_channels);
        interleaved_output.resize(duration * num_channels, O::default());

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
        let output = resampler.output_buffer_allocate(true);
        let input = vec![Vec::with_capacity(duration); num_channels];

        Ok(Self {
            resampler,
            duration,
            input,
            output,
            interleaved_output,
            channels: num_channels,
            _from_bits_per_sample,
            _to_bits_per_sample,
        })
    }

    pub fn resample(&mut self, input: &AudioBufferRef<'_>) -> Option<&[O]> {
        self.interleaved_output.fill(O::default());

        match input {
            AudioBufferRef::S16(buffer) => {
                convert_samples(buffer, &mut self.input);
                self.resampler
                    .process_into_buffer(&self.input, &mut self.output, None)
                    .unwrap();
                for (i, frame) in self
                    .interleaved_output
                    .chunks_exact_mut(self.channels)
                    .enumerate()
                {
                    for (ch, s) in frame.iter_mut().enumerate() {
                        *s = self.output[ch][i].into_sample();
                    }
                }
            }
            AudioBufferRef::U24(buffer) => {
                convert_samples(buffer, &mut self.input);
                self.resampler
                    .process_into_buffer(&self.input, &mut self.output, None)
                    .unwrap();
                for (i, frame) in self
                    .interleaved_output
                    .chunks_exact_mut(self.channels)
                    .enumerate()
                {
                    for (ch, s) in frame.iter_mut().enumerate() {
                        *s = self.output[ch][i].into_sample();
                    }
                }
            }
            AudioBufferRef::S24(buffer) => {
                convert_samples(buffer, &mut self.input);
                self.resampler
                    .process_into_buffer(&self.input, &mut self.output, None)
                    .unwrap();
                for (i, frame) in self
                    .interleaved_output
                    .chunks_exact_mut(self.channels)
                    .enumerate()
                {
                    for (ch, s) in frame.iter_mut().enumerate() {
                        *s = self.output[ch][i].into_sample();
                    }
                }
            }
            AudioBufferRef::S32(buffer) => {
                convert_samples(buffer, &mut self.input);
                self.resampler
                    .process_into_buffer(&self.input, &mut self.output, None)
                    .unwrap();
                self.interleaved_output.resize(self.channels * self.output[0].len(), O::MID);
                for (i, frame) in self
                    .interleaved_output
                    .chunks_exact_mut(self.channels)
                    .enumerate()
                {
                    for (ch, s) in frame.iter_mut().enumerate() {
                        *s = self.output[ch][i].into_sample();
                    }
                }

                for pane in self.input.iter_mut() {
                    pane.fill(0.0);
                }
            }
            AudioBufferRef::F32(buffer) => {
                let mut inputbuffer = vec![Vec::with_capacity(self.duration); self.channels];
                convert_samples(buffer, &mut inputbuffer);
                self.resampler
                    .process_into_buffer(&inputbuffer, &mut self.output, None)
                    .unwrap();
                for (i, frame) in self
                    .interleaved_output
                    .chunks_exact_mut(self.channels)
                    .enumerate()
                {
                    for (ch, s) in frame.iter_mut().enumerate() {
                        *s = self.output[ch][i].into_sample();
                    }
                }
            }
            _ => {
                println!("Unsupported sample format");
            }
        }

        Some(&self.interleaved_output)
    }
}

fn convert_samples<S>(input: &AudioBuffer<S>, output: &mut [Vec<f32>])
where
    S: Sample + IntoSample<f32>,
{
    for (c, dst) in output.iter_mut().enumerate() {
        let src = input.chan(c);
        dst.extend(src.iter().map(|&s| s.into_sample()));
    }
}
