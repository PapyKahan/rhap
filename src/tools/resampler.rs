use anyhow::Result;
use log::error;
use audioadapter_buffers::direct::SequentialSliceOfVecs;
use rubato::{Fft, FixedSync, Indexing, Resampler};
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
        let actual_frames = input.frames();

        match input {
            AudioBufferRef::S32(buffer) => {
                copy_samples_vec(buffer, &mut self.input);

                // For short packets (e.g. last packet of a track), pad input
                // to chunk_size and use partial_len instead of rebuilding
                // the FFT plan, which is expensive.
                let is_partial = actual_frames < self.frames;
                if is_partial {
                    for ch in &mut self.input {
                        ch.resize(self.frames, 0.0);
                    }
                }

                let indexing = if is_partial {
                    Some(Indexing {
                        input_offset: 0,
                        output_offset: 0,
                        partial_len: Some(actual_frames),
                        active_channels_mask: None,
                    })
                } else {
                    None
                };

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
                    indexing.as_ref(),
                )?;

                self.input.iter_mut().for_each(|channel| {
                    channel.drain(0..self.frames);
                });

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
