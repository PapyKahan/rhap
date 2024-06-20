use std::fmt::Display;

use anyhow::Result;
use libsoxr::{Datatype, IOSpec, QualityFlags, QualityRecipe, QualitySpec, RuntimeSpec, Soxr};
use rubato::{
    FftFixedIn, FftFixedInOut, VecResampler
};
use symphonia::core::{
    audio::{AudioBuffer, AudioBufferRef, Signal},
    conv::{FromSample, IntoSample},
    sample::Sample,
};

use crate::audio::BitsPerSample;

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
    internal_output: Vec<f32>,
    frames: usize,
    channels: usize,
}

impl<O> SoxrResampler<O>
where
    O: Default + Copy + Clone + Display + Sample + IntoSample<f32> + FromSample<f32>,
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
        let input_type = Datatype::Float32S;
        //let output_type = match to_bits_per_sample {
        //    BitsPerSample::Bits16 => Datatype::Int16I,
        //    BitsPerSample::Bits24 => Datatype::Int32I,
        //    BitsPerSample::Bits32 => Datatype::Float32I,
        //};
        let output_type = Datatype::Float32S;
        let io_spec = IOSpec::new(input_type, output_type);
        let runtime_spec = RuntimeSpec::new(4);
        let quality_spec = QualitySpec::new(&QualityRecipe::Low, QualityFlags::ROLLOFF_SMALL);
        let resampler = InternalSoxrResampler::create(
            from_samplerate as f64,
            to_samplerate as f64,
            channels as u32,
            Some(&io_spec),
            Some(&quality_spec),
            Some(&runtime_spec),
        )?;

        let input = vec![f32::default(); frames * channels];
        let internal_output = vec![f32::default(); frames * channels];
        let output = vec![O::default(); frames * channels];

        Ok(Self {
            resampler,
            input,
            output,
            frames,
            channels,
            internal_output,
        })
    }

    pub fn resample(&mut self, input: &AudioBufferRef<'_>) -> Option<&[O]> {
        match input {
            AudioBufferRef::S32(buffer) => {
                copy_samples_planar(buffer, &mut self.input);
                self.resampler
                    .process(Some(&self.input), &mut self.internal_output)
                    .unwrap();
                self.resampler
                    .process::<f32, f32>(None, &mut self.internal_output[0..])
                    .unwrap();
                self.resampler.0.clear().unwrap();

                self.input.drain(..self.frames * self.channels);
                self.output.resize(self.internal_output.len(), O::MID);

                for (index, frame) in self.output.chunks_exact_mut(self.channels).enumerate() {
                    for (channel, sample) in frame.iter_mut().enumerate() {
                        *sample = self.internal_output[channel * self.frames + index].into_sample();
                    }
                }

                Some(&self.output)
            }
            _ => {
                println!("Unsupported sample format");
                None
            }
        }
    }
}

pub struct RubatoResampler<O> {
    resampler: FftFixedIn<f32>,
    input: Vec<Vec<f32>>,
    output: Vec<Vec<f32>>,
    interleaved_output: Vec<O>,
    from_samplerate: usize,
    to_samplerate: usize,
    frames: usize,
    channels: usize,
}

impl<O> RubatoResampler<O>
where
    O: Sample + FromSample<f32> + IntoSample<f32> + Default + Clone,
{
    pub fn new(
        from_samplerate: usize,
        to_samplerate: usize,
        _from_bits_per_sample: BitsPerSample,
        _to_bits_per_sample: BitsPerSample,
        frames: usize,
        channels: usize,
    ) -> Result<Self> {

        let resampler = rubato::FftFixedIn::<f32>::new(
            from_samplerate,
            to_samplerate,
            frames,
            1,
            channels,
        )?;

        let output = resampler.output_buffer_allocate(true);
        let input = resampler.input_buffer_allocate(true);
        let interleaved_output = Vec::<O>::with_capacity(frames * channels);

        Ok(Self {
            resampler,
            input,
            output,
            interleaved_output,
            from_samplerate,
            to_samplerate,
            frames,
            channels,
        })
    }

    pub fn resample(&mut self, input: &AudioBufferRef<'_>) -> Option<&[O]> {
        if input.frames() != self.frames {
            println!("Resampler: input frames mismatch");
            self.frames = input.frames();

            self.resampler = rubato::FftFixedIn::<f32>::new(
                self.from_samplerate,
                self.to_samplerate,
                self.frames,
                1,
                self.channels,
            )
            .unwrap();
            self.output = self.resampler.output_buffer_allocate(true);
            self.input = self.resampler.input_buffer_allocate(true);
        }
        match input {
            AudioBufferRef::S32(buffer) => {
                copy_samples_vec(buffer, &mut self.input);
                self.resampler
                    .process_into_buffer(&self.input, &mut self.output, None)
                    .unwrap();

                self.input.iter_mut().for_each(|channel| {
                    channel.drain(0..self.frames);
                });

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
            _ => {
                println!("Unsupported sample format");
            }
        }

        Some(&self.interleaved_output)
    }
}

fn copy_samples_vec<S>(input: &AudioBuffer<S>, output: &mut [Vec<f32>])
where
    S: Sample + IntoSample<f32>,
{
    for (channel, samples) in output.iter_mut().enumerate() {
        let source = input.chan(channel);
        samples.extend(source.iter().map(|&s| s.into_sample()));
    }
}

fn copy_samples_planar<S>(input: &AudioBuffer<S>, output: &mut Vec<f32>)
where
    S: Sample + IntoSample<f32>,
{
    for channel in 0..input.spec().channels.count() {
        let source = input.chan(channel);
        output.extend(source.iter().map(|&s| s.into_sample()));
    }
}
