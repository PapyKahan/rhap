use anyhow::Result;
use rubato::{FftFixedIn, Resampler};
use symphonia::core::{
    audio::{AudioBuffer, AudioBufferRef, Signal},
    conv::{FromSample, IntoSample},
    sample::Sample,
};

use crate::audio::BitsPerSample;

pub struct RubatoResampler<O> {
    resampler: FftFixedIn<f64>,
    input: Vec<Vec<f64>>,
    output: Vec<Vec<f64>>,
    interleaved_output: Vec<O>,
    from_samplerate: usize,
    to_samplerate: usize,
    frames: usize,
    channels: usize,
}

impl<O> RubatoResampler<O>
where
    O: Sample + FromSample<f64> + IntoSample<f64> + Default + Clone,
{
    pub fn new(
        from_samplerate: usize,
        to_samplerate: usize,
        _from_bits_per_sample: BitsPerSample,
        _to_bits_per_sample: BitsPerSample,
        frames: usize,
        channels: usize,
    ) -> Result<Self> {
        let resampler =
            rubato::FftFixedIn::<f64>::new(from_samplerate, to_samplerate, frames, 1, channels)?;

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
            self.frames = input.frames();
            self.resampler = rubato::FftFixedIn::<f64>::new(
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

fn copy_samples_vec<S, T>(input: &AudioBuffer<S>, output: &mut [Vec<T>])
where
    S: Sample + IntoSample<T>,
{
    for (channel, samples) in output.iter_mut().enumerate() {
        let source = input.chan(channel);
        samples.extend(source.iter().map(|&s| s.into_sample()));
    }
}
