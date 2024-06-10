// Symphonia
// Copyright (c) 2019-2022 The Project Symphonia Developers.
//
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//

use std::sync::Arc;

use anyhow::Result;
use libsoxr::{Datatype, IOSpec, QualitySpec, RuntimeSpec, Soxr};
use rubato::Resampler;
use symphonia::core::audio::{AudioBuffer, AudioBufferRef, Signal, SignalSpec};
use symphonia::core::conv::{FromSample, IntoSample};
use symphonia::core::sample::Sample;

pub struct MySoxr(pub Soxr);

impl MySoxr {
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
unsafe impl Send for MySoxr {}
unsafe impl Sync for MySoxr {}

pub struct ResamplerUtil<T>
where
    T: Sample + IntoSample<f32> + FromSample<f32>,
{
    resampler: MySoxr,
    //resampler: rubato::FftFixedIn<f32>,
    //resampler: rubato::FastFixedIn<f32>,
    //resampler: rubato::SincFixedIn<f32>,
    input: Vec<Vec<T>>,
    interleaved: Vec<T>,
    duration: usize,
}

impl<T> ResamplerUtil<T> {
    fn resample_inner<I, O>(&mut self) -> &[O]
    where
        I: Sample + IntoSample<O> + FromSample,
    {
        // Interleave the planar samples from Rubato.
        let num_channels = self.input.len();

        let output_len = self.duration * num_channels;
        let mut output = Vec::<O>::with_capacity(output_len);

        {
            self.resampler
                .process(Some(&self.interleaved), &mut output)
                .unwrap();
        }

        // Remove consumed samples from the input buffer.
        self.input.iter_mut().for_each(|channel| {
            channel.drain(0..self.duration);
        });

        &output
    }

    pub fn new(spec: &SignalSpec, to_sample_rate: usize, duration: u64) -> Result<Self> {
        let duration = duration as usize;
        let num_channels = spec.channels.count();
        let input_buffer = vec![Vec::<T>::with_capacity(duration); num_channels];
        let io_spec = IOSpec::new(Datatype::Float32I, Datatype::Float32I);
        let resampler = MySoxr::create(
            spec.rate as f64,
            to_sample_rate as f64,
            num_channels as u32,
            Some(&io_spec),
            None,
            None,
        )?;

        Ok(Self {
            resampler,
            input: input_buffer,
            duration,
            interleaved: Default::default(),
        })
    }

    /// Resamples a planar/non-interleaved input.
    ///
    /// Returns the resampled samples in an interleaved format.
    pub fn resample<I, O>(&mut self, input: AudioBufferRef<'_>) -> Option<&[O]>
    where
        I: Sample + IntoSample<O> + FromSample,
    {
        let num_channels = self.input.len();

        self.interleaved
            .resize(num_channels * self.input[0].len(), I::MID);

        for (i, frame) in self.interleaved.chunks_exact_mut(num_channels).enumerate() {
            for (ch, s) in frame.iter_mut().enumerate() {
                *s = self.input[ch][i].into_sample();
            }
        }

        // Check if more samples are required.
        if self.input[0].len() < self.duration {
            return None;
        }

        Some(self.resample_inner::<I, O>())
    }
}
