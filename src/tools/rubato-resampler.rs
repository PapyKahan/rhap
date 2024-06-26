use rustfft::{num_complex::Complex, Fft};
use rustfft::FftPlanner;
use libsoxr::{Datatype, IOSpec, QualityFlags, QualityRecipe, QualitySpec, RuntimeSpec, Soxr};

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

pub struct FftResampler {
    input_sample_rate: usize,
    output_sample_rate: usize,
    input_bits_per_sample: BitsPerSample,
    output_bits_per_sample: BitsPerSample,
    num_channels: usize,
    num_frames: usize,
    fft_size: usize,
    fft: Arc<dyn Fft<f32>>,
    ifft: Arc<dyn Fft<f32>>,
}

impl FftResampler {
    pub fn new(
        input_sample_rate: usize,
        output_sample_rate: usize,
        input_bits_per_sample: BitsPerSample,
        output_bits_per_sample: BitsPerSample,
        num_frames: usize,
        num_channels: usize,
    ) -> Result<Self> {
        let fft_size = num_frames.next_power_of_two();
        let mut fft_planner = FftPlanner::new();
        let fft = fft_planner.plan_fft_forward(fft_size);
        let ifft = fft_planner.plan_fft_inverse(fft_size);

        Ok(FftResampler {
            input_sample_rate,
            output_sample_rate,
            input_bits_per_sample,
            output_bits_per_sample,
            num_channels,
            num_frames,
            fft_size,
            fft,
            ifft,
        })
    }

    pub fn resample(&self, input: &AudioBufferRef) -> Option<Vec<u8>> {
        let output_length =
            self.num_channels * self.num_frames * (self.output_sample_rate as usize)
                / (self.input_sample_rate as usize);
        let mut output = vec![0.0; output_length];

        // Iterate over each channel and perform resampling
        for ch in 0..self.num_channels {
            let input_channel: Vec<f32> = match input {
                AudioBufferRef::U8(buf) => {
                    buf.chan(ch).iter().map(|&s| f32::from_sample(s)).collect()
                }
                AudioBufferRef::S16(buf) => {
                    buf.chan(ch).iter().map(|&s| f32::from_sample(s)).collect()
                }
                AudioBufferRef::S24(buf) => {
                    buf.chan(ch).iter().map(|&s| f32::from_sample(s)).collect()
                }
                AudioBufferRef::S32(buf) => {
                    buf.chan(ch).iter().map(|&s| f32::from_sample(s)).collect()
                }
                AudioBufferRef::F32(buf) => buf.chan(ch).iter().copied().collect(),
                _ => panic!("Unsupported sample format"),
            };

            let mut complex_input: Vec<Complex<f32>> = input_channel
                .into_iter()
                .map(|s| Complex { re: s, im: 0.0 })
                .collect();
            complex_input.resize(self.fft_size, Complex { re: 0.0, im: 0.0 });

            self.fft.process(&mut complex_input);

            let mut complex_output = vec![Complex { re: 0.0, im: 0.0 }; self.fft_size];
            let resample_ratio = self.output_sample_rate as f32 / self.input_sample_rate as f32;
            for (i, sample) in complex_input.iter().enumerate().take(self.fft_size / 2) {
                let new_index = (i as f32 * resample_ratio) as usize;
                if new_index < self.fft_size / 2 {
                    complex_output[new_index] = *sample;
                }
            }

            self.ifft.process(&mut complex_output);

            let output_channel: Vec<f32> = complex_output.iter().map(|c| c.re).collect();
            for (i, &sample) in output_channel
                .iter()
                .enumerate()
                .take(output_length / self.num_channels)
            {
                output[i * self.num_channels + ch] = sample;
            }
        }

        Some(self.convert_output_to_bytes(output))
    }

    fn convert_output_to_bytes(&self, output: Vec<f32>) -> Vec<u8> {
        match self.output_bits_per_sample {
            BitsPerSample::Bits16 => output
                .iter()
                .flat_map(|&s| {
                    let sample = i16::from_sample(s);
                    sample.to_ne_bytes().to_vec()
                })
                .collect(),
            BitsPerSample::Bits24 => output
                .iter()
                .flat_map(|&s| {
                    let sample = i24::from_sample(s);
                    sample.to_ne_bytes().to_vec()
                })
                .collect(),
            BitsPerSample::Bits32 => output.iter().flat_map(|&s| s.to_ne_bytes()).collect(),
            _ => panic!("Unsupported output sample format"),
        }
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
