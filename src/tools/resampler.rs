use anyhow::Result;
use log::error;
use audioadapter_buffers::direct::SequentialSliceOfVecs;
use rubato::{Fft, FixedSync, Resampler};
use symphonia::core::{
    audio::{AudioBuffer, AudioBufferRef, Signal},
    conv::{FromSample, IntoSample},
    sample::Sample,
};

use crate::audio::BitsPerSample;

pub struct RubatoResampler<O> {
    resampler: Fft<f64>,
    input: Vec<Vec<f64>>,
    output: Vec<Vec<f64>>,
    interleaved_output: Vec<O>,
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
        let resampler = rubato::Fft::<f64>::new(
            from_samplerate,
            to_samplerate,
            frames,
            1,
            channels,
            FixedSync::Input,
        )?;

        let output_frames = resampler.output_frames_max();
        let input_frames = resampler.input_frames_max();
        let output = vec![vec![0.0f64; output_frames]; channels];
        let input = vec![vec![0.0f64; input_frames]; channels];
        let interleaved_output = Vec::<O>::with_capacity(frames * channels);

        Ok(Self {
            resampler,
            input,
            output,
            interleaved_output,
            frames,
            channels,
        })
    }

    pub fn resample(&mut self, input: &AudioBufferRef<'_>) -> Result<&[O]> {
        // Clear input buffers from previous iteration
        self.input.iter_mut().for_each(|ch| ch.clear());
        match input {
            AudioBufferRef::S32(buffer) => {
                copy_samples_vec(buffer, &mut self.input);

                // Pad short packets (e.g. last packet of a track) with silence
                // instead of rebuilding the FFT resampler, which is expensive.
                let actual_frames = input.frames();
                if actual_frames < self.frames {
                    for ch in &mut self.input {
                        ch.resize(self.frames, 0.0);
                    }
                }

                let input_adapter = SequentialSliceOfVecs::new(
                    &self.input,
                    self.channels,
                    self.frames,
                )?;
                let mut output_adapter = SequentialSliceOfVecs::new_mut(
                    &mut self.output,
                    self.channels,
                    self.resampler.output_frames_max(),
                )?;
                let (_in_frames, out_frames) = self.resampler.process_into_buffer(
                    &input_adapter,
                    &mut output_adapter,
                    None,
                )?;

                self.input.iter_mut().for_each(|channel| {
                    channel.drain(0..self.frames);
                });

                // Trim output to match actual input frames ratio to avoid trailing silence
                let actual_out_frames = if actual_frames < self.frames {
                    (out_frames * actual_frames) / self.frames
                } else {
                    out_frames
                };
                let out_frames = actual_out_frames;

                self.interleaved_output
                    .resize(self.channels * out_frames, O::MID);

                self.interleaved_output
                    .chunks_exact_mut(self.channels)
                    .enumerate()
                    .for_each(|(i, frame)| {
                        frame.iter_mut().enumerate().for_each(|(ch, s)| {
                            *s = self.output[ch][i].into_sample();
                        })
                    });
            }
            _ => {
                error!("Unsupported sample format");
            }
        }

        Ok(&self.interleaved_output)
    }
}

#[inline(always)]
fn copy_samples_vec<S, T>(input: &AudioBuffer<S>, output: &mut [Vec<T>])
where
    S: Sample + IntoSample<T>,
{
    output
        .iter_mut()
        .enumerate()
        .for_each(|(channel, samples)| {
            let source = input.chan(channel);
            samples.extend(source.iter().map(|&s| s.into_sample()));
        });
}
