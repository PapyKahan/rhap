use std::fmt::Display;

use anyhow::Result;
use libsoxr::{Datatype, IOSpec, QualityFlags, QualityRecipe, QualitySpec, RuntimeSpec, Soxr};
use rubato::{
    calculate_cutoff, Resampler, SincInterpolationParameters, SincInterpolationType, WindowFunction,
};
use symphonia::core::{
    audio::{AudioBuffer, AudioBufferRef, Signal},
    conv::{FromSample, IntoSample},
    sample::{i24, Sample},
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
    input: Vec<f32>,
    internal_output_buffer: Vec<f32>,
}

impl<O> SoxrResampler<O>
where
    O: Default + Copy + Clone + Display + IntoSample<O> + FromSample<f32>,
{
    pub fn new(
        from_samplerate: usize,
        to_samplerate: usize,
        _from_bits_per_sample: BitsPerSample,
        _to_bits_per_sample: BitsPerSample,
        frames: usize,
        channels: usize,
    ) -> Result<Self> {
        //let input_type = match from_bits_per_sample {
        //    BitsPerSample::Bits16 => Datatype::Int16S,
        //    BitsPerSample::Bits24 => Datatype::Int32S,
        //    BitsPerSample::Bits32 => Datatype::Float32S,
        //};
        let input_type = Datatype::Float32I;
        //let output_type = match to_bits_per_sample {
        //    BitsPerSample::Bits16 => Datatype::Int16I,
        //    BitsPerSample::Bits24 => Datatype::Int32I,
        //    BitsPerSample::Bits32 => Datatype::Float32I,
        //};
        let output_type = Datatype::Float32I;
        let io_spec = IOSpec::new(input_type, output_type);
        let runtime_spec = RuntimeSpec::new(4);
        let quality_spec = QualitySpec::new(&QualityRecipe::High, QualityFlags::ROLLOFF_SMALL);
        let resampler = InternalSoxrResampler::create(
            from_samplerate as f64,
            to_samplerate as f64,
            channels as u32,
            Some(&io_spec),
            Some(&quality_spec),
            Some(&runtime_spec),
        )?;

        let mut input = vec![f32::default(); frames * channels];
        input.resize(frames * channels, f32::default());
        let mut output = vec![O::default(); frames * channels];
        output.resize(frames * channels, O::default());
        let mut internal_output_buffer = vec![f32::default(); frames * channels];
        internal_output_buffer.resize(frames * channels, f32::default());

        Ok(Self {
            resampler,
            input,
            output,
            internal_output_buffer,
        })
    }

    pub fn resample(&mut self, input: &AudioBufferRef<'_>) -> Option<&[O]> {
        match input {
            AudioBufferRef::S16(buffer) => {
                self.input.fill(0.0);
                self.input
                    .resize(buffer.frames() * buffer.spec().channels.count(), 0.0);
                self.output.fill(O::default());
                self.output.resize(
                    buffer.frames() * buffer.spec().channels.count(),
                    O::default(),
                );
                self.internal_output_buffer.fill(0.0);
                self.internal_output_buffer
                    .resize(buffer.frames() * buffer.spec().channels.count(), 0.0);

                for (i, frame) in self
                    .input
                    .chunks_exact_mut(buffer.spec().channels.count())
                    .enumerate()
                {
                    for (channel, sample) in frame.iter_mut().enumerate() {
                        *sample = buffer.chan(channel)[i].into_sample();
                    }
                }

                self.resampler
                    .process(Some(&self.input), &mut self.internal_output_buffer)
                    .unwrap();
                self.resampler
                    .process::<f32, f32>(None, &mut self.internal_output_buffer[0..])
                    .unwrap();
                self.resampler.0.clear().unwrap();

                let mut index = 0;
                for sample in self.internal_output_buffer.iter().copied() {
                    self.output[index] = sample.into_sample();
                    index += 1;
                }

                Some(&self.output)
            }
            AudioBufferRef::S24(buffer) => {
                self.input.fill(0.0);
                self.input
                    .resize(buffer.frames() * buffer.spec().channels.count(), 0.0);
                self.output.fill(O::default());
                self.output.resize(
                    buffer.frames() * buffer.spec().channels.count(),
                    O::default(),
                );
                self.internal_output_buffer.fill(0.0);
                self.internal_output_buffer
                    .resize(buffer.frames() * buffer.spec().channels.count(), 0.0);

                for (i, frame) in self
                    .input
                    .chunks_exact_mut(buffer.spec().channels.count())
                    .enumerate()
                {
                    for (channel, sample) in frame.iter_mut().enumerate() {
                        *sample = buffer.chan(channel)[i].into_sample();
                    }
                }

                self.resampler
                    .process(Some(&self.input), &mut self.internal_output_buffer)
                    .unwrap();
                self.resampler
                    .process::<f32, f32>(None, &mut self.internal_output_buffer[0..])
                    .unwrap();
                self.resampler.0.clear().unwrap();

                let mut index = 0;
                for sample in self.internal_output_buffer.iter().copied() {
                    self.output[index] = sample.into_sample();
                    index += 1;
                }

                Some(&self.output)
            }
            AudioBufferRef::S32(buffer) => {
                self.input.fill(0.0);
                self.input
                    .resize(buffer.frames() * buffer.spec().channels.count(), 0.0);
                self.output.fill(O::default());
                self.output.resize(
                    buffer.frames() * buffer.spec().channels.count(),
                    O::default(),
                );
                self.internal_output_buffer.fill(0.0);
                self.internal_output_buffer
                    .resize(buffer.frames() * buffer.spec().channels.count(), 0.0);

                for (i, frame) in self
                    .input
                    .chunks_exact_mut(buffer.spec().channels.count())
                    .enumerate()
                {
                    for (channel, sample) in frame.iter_mut().enumerate() {
                        *sample = buffer.chan(channel)[i].into_sample();
                    }
                }

                self.resampler
                    .process(Some(&self.input), &mut self.internal_output_buffer)
                    .unwrap();
                self.resampler
                    .process::<f32, f32>(None, &mut self.internal_output_buffer[0..])
                    .unwrap();
                self.resampler.0.clear().unwrap();

                let mut index = 0;
                for sample in self.internal_output_buffer.iter().copied() {
                    self.output[index] = sample.into_sample();
                    index += 1;
                }

                Some(&self.output)
            }
            AudioBufferRef::F32(buffer) => {
                self.input.fill(0.0);
                self.input
                    .resize(buffer.frames() * buffer.spec().channels.count(), 0.0);
                self.output.fill(O::default());
                self.output.resize(
                    buffer.frames() * buffer.spec().channels.count(),
                    O::default(),
                );
                self.internal_output_buffer.fill(0.0);
                self.internal_output_buffer
                    .resize(buffer.frames() * buffer.spec().channels.count(), 0.0);

                for (i, frame) in self
                    .input
                    .chunks_exact_mut(buffer.spec().channels.count())
                    .enumerate()
                {
                    for (channel, sample) in frame.iter_mut().enumerate() {
                        *sample = buffer.chan(channel)[i].into_sample();
                    }
                }

                self.resampler
                    .process(Some(&self.input), &mut self.internal_output_buffer)
                    .unwrap();
                self.resampler
                    .process::<f32, f32>(None, &mut self.internal_output_buffer[0..])
                    .unwrap();
                self.resampler.0.clear().unwrap();

                let mut index = 0;
                for sample in self.internal_output_buffer.iter().copied() {
                    self.output[index] = sample.into_sample();
                    index += 1;
                }

                Some(&self.output)
            }
            _ => None,
        }
    }
}

pub struct RubatoResampler<O> {
    resampler: rubato::SincFixedIn<f32>,
    //resampler: rubato::FftFixedIn<f32>,
    input: Vec<Vec<f32>>,
    output: Vec<Vec<f32>>,
    interleaved_output: Vec<O>,
    frames: usize,
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
        frames: u64,
        channels: usize,
    ) -> Result<Self> {
        let mut interleaved_output = Vec::<O>::with_capacity(frames as usize * channels);
        interleaved_output.resize(frames as usize * channels, O::default());

        let ratio = to_samplerate as f32 / from_samplerate as f32;
        let sinc_len = 256;
        let oversampling_factor = sinc_len;
        let interpolation = SincInterpolationType::Quadratic;
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
            rubato::SincFixedIn::<f32>::new(ratio as f64, 1.0, params, frames as usize, channels)
                .unwrap();

        //let subchunk_size = if from_samplerate as usize > to_samplerate {
        //    frames as usize / (from_samplerate / to_samplerate)
        //} else {
        //    frames as usize / (to_samplerate / from_samplerate)
        //};

        //let resampler = rubato::FftFixedIn::<f32>::new(
        //    from_samplerate,
        //    to_samplerate,
        //    frames as usize,
        //    subchunk_size,
        //    channels,
        //)?;

        let output = resampler.output_buffer_allocate(true);
        let input = vec![Vec::with_capacity(frames as usize); channels];

        Ok(Self {
            resampler,
            frames: frames as usize,
            input,
            output,
            interleaved_output,
            channels,
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
                for channel in self.input.iter_mut() {
                    channel.resize(self.frames, 0.0);
                }

                convert_samples(buffer, &mut self.input);

                self.resampler
                    .process_into_buffer(&self.input, &mut self.output, None)
                    .unwrap();

                self.interleaved_output
                    .resize(self.channels * self.output[0].len(), O::MID);
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
            AudioBufferRef::F32(buffer) => {
                let mut inputbuffer = vec![Vec::with_capacity(self.frames); self.channels];
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
        for (index, sample) in src.iter().map(|&s| s.into_sample()).enumerate() {
            dst[index] = sample;
        }
        //dst.extend(src.iter().map(|&s| s.into_sample()));
    }
}
