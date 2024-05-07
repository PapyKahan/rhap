// Symphonia
// Copyright (c) 2019-2022 The Project Symphonia Developers.
//
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//

use rubato::{calculate_cutoff, Resampler, SincInterpolationParameters, SincInterpolationType, WindowFunction};
use symphonia::core::audio::{AudioBuffer, AudioBufferRef, Signal, SignalSpec};
use symphonia::core::conv::{FromSample, IntoSample};
use symphonia::core::sample::Sample;

use rubato::sinc_interpolator::sinc_interpolator_avx::AvxInterpolator;

pub struct ResamplerUtil<T> {
    resampler: rubato::FftFixedIn<f64>,
    //resampler: rubato::FastFixedIn<f64>,
    //resampler: rubato::SincFixedIn<f64>,
    input: Vec<Vec<f64>>,
    output: Vec<Vec<f64>>,
    interleaved: Vec<T>,
    duration: usize,
}

impl<T> ResamplerUtil<T>
where
    T: Sample + FromSample<f64> + IntoSample<f64>,
{
    fn resample_inner(&mut self) -> &[T] {
        {
            let mut input: arrayvec::ArrayVec<&[f64], 32> = Default::default();
            for channel in self.input.iter() {
                input.push(&channel[..self.duration]);
            }

            self.resampler
                .process_into_buffer(&input, &mut self.output, None)
                .unwrap();
        }

        // Remove consumed samples from the input buffer.
        for channel in self.input.iter_mut() {
            channel.drain(0..self.duration);
        }

        // Interleave the planar samples from Rubato.
        let num_channels = self.output.len();

        self.interleaved
            .resize(num_channels * self.output[0].len(), T::MID);

        for (i, frame) in self.interleaved.chunks_exact_mut(num_channels).enumerate() {
            for (ch, s) in frame.iter_mut().enumerate() {
                *s = self.output[ch][i].into_sample();
            }
        }

        &self.interleaved
    }
}

impl<T> ResamplerUtil<T>
where
    T: Sample + FromSample<f64> + IntoSample<f64>,
{
    pub fn new(spec: &SignalSpec, to_sample_rate: usize, duration: u64) -> Self {
        let duration = duration as usize;
        let num_channels = 2;

        //let ratio = to_sample_rate as f64 / spec.rate as f64;
        //let sinc_len = 256;
        //let oversampling_factor = sinc_len;
        //let interpolation_type = SincInterpolationType::Nearest;
        //let window = WindowFunction::BlackmanHarris2;

        //let f_cutoff = calculate_cutoff(sinc_len, window);
        //let f_cutoff = 0.80;
        //let params = SincInterpolationParameters {
        //    sinc_len,
        //    f_cutoff,
        //    interpolation,
        //    oversampling_factor,
        //    window,
        //};

        //let resampler = rubato::FastFixedIn::<f64>::new(ratio, 1.1, rubato::PolynomialDegree::Quintic, duration, num_channels).unwrap();

        //let subchunk_size = if spec.rate as usize > to_sample_rate {
        //    duration / (spec.rate as usize / to_sample_rate)
        //} else {
        //    duration / (to_sample_rate / spec.rate as usize)
        //};
        
        //let resampler = rubato::SincFixedIn::<f64>::new(ratio, 1.0, params, duration, num_channels).unwrap();
        //let avx = AvxInterpolator::new(sinc_len, oversampling_factor, f_cutoff, window).unwrap();
        //let interpolator = Box::new(avx);
        //let resampler = rubato::SincFixedIn::<f64>::new_with_interpolator(ratio, 1.1, interpolation_type, interpolator, duration, num_channels).unwrap();

        let resampler = rubato::FftFixedIn::<f64>::new(
            spec.rate as usize,
            to_sample_rate,
            duration,
            duration,
            num_channels,
        )
        .unwrap();

        let output_buffer = resampler.output_buffer_allocate(true);
        let input_buffer = vec![Vec::with_capacity(duration); num_channels];

        Self {
            resampler,
            input: input_buffer,
            output: output_buffer,
            duration,
            interleaved: Default::default(),
        }
    }

    /// Resamples a planar/non-interleaved input.
    ///
    /// Returns the resampled samples in an interleaved format.
    pub fn resample(&mut self, input: AudioBufferRef<'_>) -> Option<&[T]> {
        // Copy and convert samples into input buffer.
        convert_samples_any(&input, &mut self.input);

        // Check if more samples are required.
        if self.input[0].len() < self.duration {
            return None;
        }

        Some(self.resample_inner())
    }

    /// Resample any remaining samples in the resample buffer.
    pub fn flush(&mut self) -> Option<&[T]> {
        let len = self.input[0].len();

        if len == 0 {
            return None;
        }

        let partial_len = len % self.duration;

        if partial_len != 0 {
            // Fill each input channel buffer with silence to the next multiple of the resampler
            // duration.
            for channel in self.input.iter_mut() {
                channel.resize(len + (self.duration - partial_len), f64::MID);
            }
        }

        Some(self.resample_inner())
    }
}

fn convert_samples_any(input: &AudioBufferRef<'_>, output: &mut [Vec<f64>]) {
    match input {
        AudioBufferRef::U8(input) => convert_samples(input, output),
        AudioBufferRef::U16(input) => convert_samples(input, output),
        AudioBufferRef::U24(input) => convert_samples(input, output),
        AudioBufferRef::U32(input) => convert_samples(input, output),
        AudioBufferRef::S8(input) => convert_samples(input, output),
        AudioBufferRef::S16(input) => convert_samples(input, output),
        AudioBufferRef::S24(input) => convert_samples(input, output),
        AudioBufferRef::S32(input) => convert_samples(input, output),
        AudioBufferRef::F32(input) => convert_samples(input, output),
        AudioBufferRef::F64(input) => convert_samples(input, output),
    }
}

fn convert_samples<S>(input: &AudioBuffer<S>, output: &mut [Vec<f64>])
where
    S: Sample + IntoSample<f64>,
{
    for (c, dst) in output.iter_mut().enumerate() {
        let src = input.chan(c);
        dst.extend(src.iter().map(|&s| s.into_sample()));
    }
}
