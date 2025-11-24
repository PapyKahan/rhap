#[cfg(target_os = "linux")]
use alsa::pcm::{PCM, HwParams, Format, Access, State};
#[cfg(target_os = "linux")]
use alsa::{Direction, ValueOr};
#[cfg(target_os = "linux")]
use anyhow::{Result, anyhow};
#[cfg(target_os = "linux")]
use crate::audio::{SampleRate, BitsPerSample};
#[cfg(target_os = "linux")]
use std::ffi::CString;
#[cfg(target_os = "linux")]
use crate::logging::log_to_file_only;

#[cfg(target_os = "linux")]
pub struct AlsaPcm {
    pcm: PCM,
    sample_rate: u32,
    channels: u32,
    format: Format,
    period_size: u32,
    buffer_size: u32,
    requested_format: Format, // Track what was originally requested
    conversion_buffer: Vec<u8>, // Buffer for handling incomplete frames during conversion
}

#[cfg(target_os = "linux")]
impl AlsaPcm {
    pub fn open(device_name: &str, sample_rate: SampleRate, channels: u8, bits_per_sample: BitsPerSample, exclusive: bool) -> Result<Self> {
        // Open PCM device
        let device_cstr = CString::new(device_name)
            .map_err(|e| anyhow!("Invalid device name: {}", e))?;
        let pcm = PCM::open(&device_cstr, Direction::Playback, false)
            .map_err(|e| anyhow!("Failed to open ALSA device {}: {}", device_name, e))?;

        // Set format based on bits per sample
        let sample_format = match bits_per_sample {
            BitsPerSample::Bits16 => Format::S16LE,
            BitsPerSample::Bits24 => Format::S243LE,
            BitsPerSample::Bits32 => Format::S32LE,
        };

        log_to_file_only("ALSA", &format!("Requesting format: {:?} for {}-bit audio", sample_format, bits_per_sample as u32));

        {
            // Set hardware parameters
            let hw_params = HwParams::any(&pcm)
                .map_err(|e| anyhow!("Failed to get hardware parameters: {}", e))?;

            // Set access mode
            let access_mode = if exclusive {
                Access::RWInterleaved
            } else {
                Access::RWInterleaved
            };
            log_to_file_only("ALSA", &format!("Setting access mode: {:?}", access_mode));
            hw_params.set_access(access_mode)
                .map_err(|e| anyhow!("Failed to set access mode: {}", e))?;

            log_to_file_only("ALSA", &format!("Attempting to set format: {:?} for {}-bit audio", sample_format, bits_per_sample as u32));
            match hw_params.set_format(sample_format) {
                Ok(()) => {
                    log_to_file_only("ALSA", "Format set successfully");
                }
                Err(e) => {
                    log_to_file_only("ALSA", &format!("Failed to set requested format {:?}: {}", sample_format, e));

                    // Try fallback formats for 24-bit
                    if bits_per_sample == BitsPerSample::Bits24 {
                        log_to_file_only("ALSA", "Trying fallback to 16-bit format for 24-bit request");
                        match hw_params.set_format(Format::S16LE) {
                            Ok(()) => {
                                log_to_file_only("ALSA", "Fallback to 16-bit successful");
                            }
                            Err(e2) => {
                                log_to_file_only("ALSA", &format!("Fallback to 16-bit also failed: {}", e2));
                                return Err(anyhow!("Failed to set both 24-bit and 16-bit formats"));
                            }
                        }
                    } else {
                        return Err(anyhow!("Failed to set format: {}", e));
                    }
                }
            }

            // Set channels
            log_to_file_only("ALSA", &format!("Setting channels: {}", channels));
            hw_params.set_channels(channels as u32)
                .map_err(|e| anyhow!("Failed to set channels: {}", e))?;

            // Set sample rate
            let rate = sample_rate as u32;
            log_to_file_only("ALSA", &format!("Setting sample rate: {}", rate));
            let actual_rate = hw_params.set_rate_near(rate, ValueOr::Nearest)
                .map_err(|e| anyhow!("Failed to set sample rate: {}", e))?;
            if actual_rate != rate {
                log_to_file_only("ALSA", &format!("Sample rate adjusted from {} to {}", rate, actual_rate));
            }

            // Set period and buffer sizes
            let period_size = 1024; // Frames per period
            let buffer_size = period_size * 4; // 4 periods in buffer
            log_to_file_only("ALSA", &format!("Setting period_size: {}, buffer_size: {}", period_size, buffer_size));

            hw_params.set_period_size(period_size, ValueOr::Nearest)
                .map_err(|e| anyhow!("Failed to set period size: {}", e))?;

            hw_params.set_buffer_size(buffer_size)
                .map_err(|e| anyhow!("Failed to set buffer size: {}", e))?;

            // Apply hardware parameters
            log_to_file_only("ALSA", "Applying hardware parameters...");
            pcm.hw_params(&hw_params)
                .map_err(|e| anyhow!("Failed to apply hardware parameters: {}", e))?;
            log_to_file_only("ALSA", "Hardware parameters applied successfully");
        }

        // Get actual parameters that were set
        let (actual_rate, actual_channels, actual_format, actual_period_size, actual_buffer_size) = {
            let actual_hw_params = pcm.hw_params_current()
                .map_err(|e| anyhow!("Failed to get current hardware parameters: {}", e))?;

            let actual_rate = actual_hw_params.get_rate()
                .map_err(|e| anyhow!("Failed to get sample rate: {}", e))?;
            let actual_channels = actual_hw_params.get_channels()
                .map_err(|e| anyhow!("Failed to get channels: {}", e))?;
            let actual_period_size = actual_hw_params.get_period_size()
                .map_err(|e| anyhow!("Failed to get period size: {}", e))?;
            let actual_buffer_size = actual_hw_params.get_buffer_size()
                .map_err(|e| anyhow!("Failed to get buffer size: {}", e))?;

            // Get the actual format that was set
            let actual_format = actual_hw_params.get_format()
                .map_err(|e| anyhow!("Failed to get format: {}", e))?;

            log_to_file_only("ALSA", &format!("Hardware parameters - Rate: {}, Channels: {}, Format: {:?}, Period: {}, Buffer: {}",
                actual_rate, actual_channels, actual_format, actual_period_size, actual_buffer_size));

            (actual_rate, actual_channels, actual_format, actual_period_size, actual_buffer_size)
        };

        // Skip software parameters for now - use defaults

        // Prepare the PCM
        pcm.prepare()
            .map_err(|e| anyhow!("Failed to prepare PCM: {}", e))?;

        // Test if the IO interface works with the selected format
        let (final_pcm, final_format) = if actual_format == Format::S243LE {
            log_to_file_only("ALSA", "Testing IO interface for 24-bit format...");
            {
                let io_test = pcm.io_i32();
                match io_test {
                    Ok(_) => {
                        log_to_file_only("ALSA", "24-bit IO interface test passed");
                        drop(io_test);
                        (pcm, actual_format)
                    }
                    Err(e) => {
                        log_to_file_only("ALSA", &format!("24-bit IO interface test failed: {}", e));
                        drop(io_test);
                        log_to_file_only("ALSA", "Attempting fallback to 16-bit format...");

                        // Need to reopen and fallback to 16-bit
                        drop(pcm);
                        let fallback_pcm = PCM::open(&device_cstr, Direction::Playback, false)
                            .map_err(|e| anyhow!("Failed to reopen ALSA device for fallback: {}", e))?;

                        {
                            let hw_params = HwParams::any(&fallback_pcm)
                                .map_err(|e| anyhow!("Failed to get fallback hardware parameters: {}", e))?;

                            hw_params.set_access(Access::RWInterleaved)
                                .map_err(|e| anyhow!("Failed to set fallback access mode: {}", e))?;

                            hw_params.set_format(Format::S16LE)
                                .map_err(|e| anyhow!("Failed to set fallback format: {}", e))?;

                            hw_params.set_channels(channels as u32)
                                .map_err(|e| anyhow!("Failed to set fallback channels: {}", e))?;

                            hw_params.set_rate_near(sample_rate as u32, ValueOr::Nearest)
                                .map_err(|e| anyhow!("Failed to set fallback sample rate: {}", e))?;

                            hw_params.set_period_size(1024, ValueOr::Nearest)
                                .map_err(|e| anyhow!("Failed to set fallback period size: {}", e))?;

                            hw_params.set_buffer_size(4096)
                                .map_err(|e| anyhow!("Failed to set fallback buffer size: {}", e))?;

                            fallback_pcm.hw_params(&hw_params)
                                .map_err(|e| anyhow!("Failed to apply fallback hardware parameters: {}", e))?;
                        }

                        fallback_pcm.prepare()
                            .map_err(|e| anyhow!("Failed to prepare fallback PCM: {}", e))?;

                        // Test the 16-bit IO interface
                        {
                            let fallback_io_test = fallback_pcm.io_i16();
                            match fallback_io_test {
                                Ok(_) => {
                                    log_to_file_only("ALSA", "16-bit fallback IO interface test passed");
                                    drop(fallback_io_test);
                                    (fallback_pcm, Format::S16LE)
                                }
                                Err(e) => {
                                    log_to_file_only("ALSA", &format!("16-bit fallback IO interface also failed: {}", e));
                                    drop(fallback_io_test);
                                    return Err(anyhow!("Both 24-bit and 16-bit IO interfaces failed"));
                                }
                            }
                        }
                    }
                }
            }
        } else {
            (pcm, actual_format)
        };

        Ok(Self {
            pcm: final_pcm,
            sample_rate: actual_rate,
            channels: actual_channels,
            format: final_format,
            period_size: actual_period_size as u32,
            buffer_size: actual_buffer_size as u32,
            requested_format: sample_format, // Store what was originally requested
            conversion_buffer: Vec::new(), // Initialize empty conversion buffer
        })
    }

    pub fn write_bytes(&mut self, data: &[u8]) -> Result<()> {
        match self.format {
            Format::S16LE => {
                // Convert data based on what was requested vs what we have
                let samples = if self.requested_format == Format::S243LE {
                    // We requested 24-bit but got 16-bit - convert incoming 24-bit data
                    log_to_file_only("ALSA", &format!("Converting {} bytes of 24-bit audio data to 16-bit for output", data.len()));
                    log_to_file_only("ALSA", &format!("Buffer size before: {} bytes", self.conversion_buffer.len()));

                    // Add incoming data to conversion buffer and convert complete frames
                    self.conversion_buffer.extend_from_slice(data);
                    self.convert_buffered_24bit_to_16bit()?
                } else if self.requested_format == Format::S32LE {
                    // We requested 32-bit but got 16-bit - convert incoming 32-bit data
                    log_to_file_only("ALSA", &format!("Converting {} bytes of 32-bit audio data to 16-bit for output", data.len()));
                    self.convert_32bit_to_16bit(data)?
                } else {
                    // Regular 16-bit processing
                    log_to_file_only("ALSA", &format!("Processing {} bytes as 16-bit audio data", data.len()));
                    data.chunks_exact(2)
                        .map(|chunk| i16::from_le_bytes([chunk[0], chunk[1]]))
                        .collect()
                };

                let io = self.pcm.io_i16()
                    .map_err(|e| anyhow!("Failed to get PCM IO i16: {}", e))?;

                let frames_to_write = samples.len() / self.channels as usize;
                let samples_written = io.writei(&samples)
                    .map_err(|e| anyhow!("Failed to write i16 audio data: {}", e))?;

                if samples_written != frames_to_write {
                    return Err(anyhow!("Partial i16 write: expected {}, wrote {}", frames_to_write, samples_written));
                }
            }
            Format::S243LE => {
                let io = self.pcm.io_i32()
                    .map_err(|e| anyhow!("Failed to get PCM IO i32: {}", e))?;

                // Convert 24-bit packed to 32-bit samples
                let frames_to_write = data.len() / (self.channels as usize * 3); // 3 bytes per 24-bit sample
                let samples: Vec<i32> = data.chunks_exact(3)
                    .map(|chunk| {
                        // Convert 3 bytes to i32 (24-bit little-endian)
                        let value = ((chunk[2] as i32) << 16) | ((chunk[1] as i32) << 8) | (chunk[0] as i32);
                        // Sign extend for negative values (24-bit to 32-bit)
                        if chunk[2] >= 0x80 {
                            value | -16777216i32  // 0xFF000000
                        } else {
                            value
                        }
                    })
                    .collect();

                let samples_written = io.writei(&samples)
                    .map_err(|e| anyhow!("Failed to write i32 audio data: {}", e))?;

                if samples_written != frames_to_write {
                    return Err(anyhow!("Partial i32 write: expected {}, wrote {}", frames_to_write, samples_written));
                }
            }
            Format::S32LE => {
                let io = self.pcm.io_i32()
                    .map_err(|e| anyhow!("Failed to get PCM IO i32: {}", e))?;

                // Convert bytes to i32 samples
                let frames_to_write = data.len() / (self.channels as usize * 4); // 4 bytes per i32 sample
                let samples: Vec<i32> = data.chunks_exact(4)
                    .map(|chunk| i32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
                    .collect();

                let samples_written = io.writei(&samples)
                    .map_err(|e| anyhow!("Failed to write i32 audio data: {}", e))?;

                if samples_written != frames_to_write {
                    return Err(anyhow!("Partial i32 write: expected {}, wrote {}", frames_to_write, samples_written));
                }
            }
            _ => {
                return Err(anyhow!("Unsupported ALSA format: {:?}", self.format));
            }
        }

        Ok(())
    }

    pub fn get_sample_rate(&self) -> u32 {
        self.sample_rate
    }

    pub fn get_channels(&self) -> u32 {
        self.channels
    }

    pub fn get_period_size(&self) -> u32 {
        self.period_size
    }

    pub fn drain(&mut self) -> Result<()> {
        self.pcm.drain()
            .map_err(|e| anyhow!("Failed to drain PCM: {}", e))?;
        Ok(())
    }

    pub fn pause(&mut self) -> Result<()> {
        self.pcm.pause(true)
            .map_err(|e| anyhow!("Failed to pause PCM: {}", e))?;
        Ok(())
    }

    pub fn resume(&mut self) -> Result<()> {
        self.pcm.pause(false)
            .map_err(|e| anyhow!("Failed to resume PCM: {}", e))?;
        Ok(())
    }

    fn convert_24bit_to_16bit(&self, data: &[u8]) -> Result<Vec<i16>> {
        let bytes_per_sample_24 = 3;
        let frames = data.len() / (self.channels as usize * bytes_per_sample_24);
        let mut samples = Vec::with_capacity(frames * self.channels as usize);

        for frame in 0..frames {
            for channel in 0..self.channels {
                let sample_offset = (frame * self.channels as usize + channel as usize) * bytes_per_sample_24;

                if sample_offset + bytes_per_sample_24 <= data.len() {
                    // Convert 24-bit little-endian to i32
                    let value = ((data[sample_offset + 2] as i32) << 16) |
                                ((data[sample_offset + 1] as i32) << 8) |
                                (data[sample_offset] as i32);

                    // Sign extend for negative values (24-bit to 32-bit)
                    let value_24 = if data[sample_offset + 2] >= 0x80 {
                        value | -16777216i32  // 0xFF000000
                    } else {
                        value
                    };

                    // Convert 24-bit to 16-bit by shifting right 8 bits
                    // This preserves the dynamic range while fitting in 16-bit
                    let value_16 = (value_24 >> 8).clamp(i16::MIN as i32, i16::MAX as i32) as i16;
                    samples.push(value_16);
                }
            }
        }

        Ok(samples)
    }

    /// Convert buffered 24-bit data to 16-bit, handling frame alignment
    fn convert_buffered_24bit_to_16bit(&mut self) -> Result<Vec<i16>> {
        let bytes_per_sample_24 = 3;
        let bytes_per_frame = self.channels as usize * bytes_per_sample_24;

        // Only convert complete frames
        let complete_frames = self.conversion_buffer.len() / bytes_per_frame;
        let bytes_to_convert = complete_frames * bytes_per_frame;

        log_to_file_only("ALSA", &format!("Buffer: {} bytes, complete frames: {}, bytes to convert: {}, remainder: {}",
            self.conversion_buffer.len(), complete_frames, bytes_to_convert,
            self.conversion_buffer.len() - bytes_to_convert));

        if complete_frames == 0 {
            return Ok(Vec::new()); // Not enough data for a complete frame
        }

        let mut samples = Vec::with_capacity(complete_frames * self.channels as usize);

        for frame in 0..complete_frames {
            for channel in 0..self.channels {
                let sample_offset = (frame * self.channels as usize + channel as usize) * bytes_per_sample_24;

                if sample_offset + bytes_per_sample_24 <= self.conversion_buffer.len() {
                    // Convert 24-bit little-endian to i32
                    let value = ((self.conversion_buffer[sample_offset + 2] as i32) << 16) |
                                ((self.conversion_buffer[sample_offset + 1] as i32) << 8) |
                                (self.conversion_buffer[sample_offset] as i32);

                    // Sign extend for negative values (24-bit to 32-bit)
                    let value_24 = if self.conversion_buffer[sample_offset + 2] >= 0x80 {
                        value | -16777216i32  // 0xFF000000
                    } else {
                        value
                    };

                    // Convert 24-bit to 16-bit by shifting right 8 bits
                    // This preserves the dynamic range while fitting in 16-bit
                    let value_16 = (value_24 >> 8).clamp(i16::MIN as i32, i16::MAX as i32) as i16;
                    samples.push(value_16);
                }
            }
        }

        // Remove the converted data from buffer, keeping remainder for next time
        self.conversion_buffer.drain(0..bytes_to_convert);

        log_to_file_only("ALSA", &format!("Converted {} samples, buffer now has {} bytes remaining",
            samples.len(), self.conversion_buffer.len()));

        Ok(samples)
    }

    /// Convert 32-bit little-endian audio data to 16-bit
    fn convert_32bit_to_16bit(&self, data: &[u8]) -> Result<Vec<i16>> {
        let bytes_per_sample_32 = 4;
        let frames = data.len() / (self.channels as usize * bytes_per_sample_32);
        let mut samples = Vec::with_capacity(frames * self.channels as usize);

        for frame in 0..frames {
            for channel in 0..self.channels {
                let sample_offset = (frame * self.channels as usize + channel as usize) * bytes_per_sample_32;

                if sample_offset + bytes_per_sample_32 <= data.len() {
                    // Convert 32-bit little-endian to i32
                    let value_32 = i32::from_le_bytes([
                        data[sample_offset],
                        data[sample_offset + 1],
                        data[sample_offset + 2],
                        data[sample_offset + 3],
                    ]);

                    // Convert 32-bit to 16-bit by shifting right 16 bits
                    let value_16 = (value_32 >> 16).clamp(i16::MIN as i32, i16::MAX as i32) as i16;
                    samples.push(value_16);
                }
            }
        }

        Ok(samples)
    }

    /// Convert 32-bit little-endian audio data to 24-bit packed format
    fn convert_32bit_to_24bit(&self, data: &[u8]) -> Result<Vec<u8>> {
        let bytes_per_sample_32 = 4;
        let bytes_per_sample_24 = 3;
        let frames = data.len() / (self.channels as usize * bytes_per_sample_32);
        let mut output = Vec::with_capacity(frames * self.channels as usize * bytes_per_sample_24);

        for frame in 0..frames {
            for channel in 0..self.channels {
                let sample_offset = (frame * self.channels as usize + channel as usize) * bytes_per_sample_32;

                if sample_offset + bytes_per_sample_32 <= data.len() {
                    // Convert 32-bit little-endian to i32
                    let value_32 = i32::from_le_bytes([
                        data[sample_offset],
                        data[sample_offset + 1],
                        data[sample_offset + 2],
                        data[sample_offset + 3],
                    ]);

                    // Convert to 24-bit packed little-endian (shift right 8 bits)
                    let value_24 = (value_32 >> 8) & 0x00FFFFFF;

                    // Pack 24-bit into 3 bytes
                    output.push((value_24 & 0xFF) as u8);
                    output.push(((value_24 >> 8) & 0xFF) as u8);
                    output.push(((value_24 >> 16) & 0xFF) as u8);
                }
            }
        }

        Ok(output)
    }

    pub fn get_state(&self) -> State {
        self.pcm.state()
    }
}