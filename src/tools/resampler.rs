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
            from_samplerate,
            to_samplerate,
            frames,
            channels,
        })
    }

    pub fn resample(&mut self, input: &AudioBufferRef<'_>) -> Result<&[O]> {
        if input.frames() != self.frames {
            self.frames = input.frames();
            self.resampler = rubato::Fft::<f64>::new(
                self.from_samplerate,
                self.to_samplerate,
                self.frames,
                1,
                self.channels,
                FixedSync::Input,
            )
            .unwrap();
            let output_frames = self.resampler.output_frames_max();
            let input_frames = self.resampler.input_frames_max();
            self.output = vec![vec![0.0f64; output_frames]; self.channels];
            self.input = vec![vec![0.0f64; input_frames]; self.channels];
        }
        match input {
            AudioBufferRef::U8(buffer) => {
                copy_samples_vec(buffer, &mut self.input);
                self.process_and_interleave()?;
            }
            AudioBufferRef::U16(buffer) => {
                copy_samples_vec(buffer, &mut self.input);
                self.process_and_interleave()?;
            }
            AudioBufferRef::U24(buffer) => {
                copy_samples_vec(buffer, &mut self.input);
                self.process_and_interleave()?;
            }
            AudioBufferRef::U32(buffer) => {
                copy_samples_vec(buffer, &mut self.input);
                self.process_and_interleave()?;
            }
            AudioBufferRef::S16(buffer) => {
                copy_samples_vec(buffer, &mut self.input);
                self.process_and_interleave()?;
            }
            AudioBufferRef::S24(buffer) => {
                copy_samples_vec(buffer, &mut self.input);
                self.process_and_interleave()?;
            }
            AudioBufferRef::S32(buffer) => {
                copy_samples_vec(buffer, &mut self.input);
                self.process_and_interleave()?;
            }
            AudioBufferRef::F32(buffer) => {
                copy_samples_vec(buffer, &mut self.input);
                self.process_and_interleave()?;
            }
            AudioBufferRef::F64(buffer) => {
                copy_samples_vec(buffer, &mut self.input);
                self.process_and_interleave()?;
            }
            _ => {
                error!("Unsupported sample format variant");
                return Err(anyhow::anyhow!("Unsupported sample format"));
            }
        }

        Ok(&self.interleaved_output)
    }

    /// Helper method to process audio through resampler and interleave output
    fn process_and_interleave(&mut self) -> Result<()> {
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

        // Clean up input buffer
        self.input.iter_mut().for_each(|channel| {
            channel.drain(0..self.frames);
        });

        // Resize output buffer for interleaved data
        self.interleaved_output
            .resize(self.channels * out_frames, O::MID);

        // Interleave channels
        self.interleaved_output
            .chunks_exact_mut(self.channels)
            .enumerate()
            .for_each(|(i, frame)| {
                frame.iter_mut().enumerate().for_each(|(ch, s)| {
                    *s = self.output[ch][i].into_sample();
                })
            });

        Ok(())
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
