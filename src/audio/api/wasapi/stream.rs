use anyhow::{anyhow, Result};
use log::debug;
use log::error;
use std::sync::mpsc::Receiver;
use std::sync::Condvar;
use std::sync::Mutex;
use std::time::Duration;
use wasapi::calculate_period_100ns;
use wasapi::AudioClient;
use wasapi::AudioRenderClient;
use wasapi::Direction;
use wasapi::Handle;
use wasapi::ShareMode;
use wasapi::WaveFormat;
use windows::Win32::Foundation::E_INVALIDARG;
use windows::Win32::Media::Audio::AUDCLNT_E_BUFFER_SIZE_NOT_ALIGNED;
use windows::Win32::Media::Audio::AUDCLNT_E_DEVICE_IN_USE;
use windows::Win32::Media::Audio::AUDCLNT_E_ENDPOINT_CREATE_FAILED;
use windows::Win32::Media::Audio::AUDCLNT_E_EXCLUSIVE_MODE_NOT_ALLOWED;
use windows::Win32::Media::Audio::AUDCLNT_E_UNSUPPORTED_FORMAT;

use super::com::com_initialize;
use super::device::Device;
use crate::audio::{StreamParams, StreamingCommand};

pub struct Streamer {
    client: AudioClient,
    renderer: AudioRenderClient,
    eventhandle: Handle,
    wave_format: WaveFormat,
    pause_condition: Condvar,
    status: Mutex<StreamingCommand>,
    receiver: Receiver<StreamingCommand>,
}

unsafe impl Send for Streamer {}
unsafe impl Sync for Streamer {}

impl Streamer {
    // WAVEFORMATEX documentation: https://learn.microsoft.com/en-us/windows/win32/api/mmreg/ns-mmreg-waveformatex
    // WAVEFORMATEXTENSIBLE documentation: https://docs.microsoft.com/en-us/windows/win32/api/mmreg/ns-mmreg-waveformatextensible
    #[inline(always)]
    pub(super) fn create_waveformat_from(params: StreamParams) -> WaveFormat {
        let sample_type = match params.bits_per_sample {
            crate::audio::BitsPerSample::Bits8 => &wasapi::SampleType::Int,
            crate::audio::BitsPerSample::Bits16 => &wasapi::SampleType::Int,
            crate::audio::BitsPerSample::Bits24 => &wasapi::SampleType::Int,
            crate::audio::BitsPerSample::Bits32 => &wasapi::SampleType::Float,
        };
        return WaveFormat::new(
            params.bits_per_sample as usize,
            params.bits_per_sample as usize,
            sample_type,
            params.samplerate as usize,
            params.channels as usize,
            None,
        );
    }

    pub(super) fn new(
        device: &Device,
        receiver: Receiver<StreamingCommand>,
        params: StreamParams,
    ) -> Result<Self> {
        com_initialize();
        let mut client = device
            .inner_device
            .get_iaudioclient()
            .map_err(|e| anyhow!("IAudioClient::GetAudioClient failed: {}", e))?;
        let wave_format = Streamer::create_waveformat_from(params.clone());
        let sharemode = match params.exclusive {
            true => ShareMode::Exclusive,
            false => ShareMode::Shared,
        };

        let (default_device_period, _) = client
            .get_periods()
            .map_err(|e| anyhow!("IAudioClient::GetDevicePeriod failed: {}", e))?;
        let default_device_period = if params.buffer_length != 0 {
            (params.buffer_length * 1000000) / 100 as i64
        } else {
            default_device_period
        };

        // Calculate desired period for better device compatibility. For our use case we don't
        // care about having a low lattency playback thats why we don't use minimum device period.
        let desired_period = client
            .calculate_aligned_period_near(3 * default_device_period / 2, Some(128), &wave_format)
            .map_err(|e| anyhow!("IAudioClient::CalculateAlignedPeriod failed: {}", e))?;
        let result = client.initialize_client(
            &wave_format,
            desired_period,
            &device.inner_device.get_direction(),
            &sharemode,
            false,
        );

        match result {
            Ok(()) => debug!("IAudioClient::Initialize ok"),
            Err(e) => {
                if let Some(werr) = e.downcast_ref::<windows::core::Error>() {
                    // Some of the possible errors. See the documentation for the full list and descriptions.
                    // https://docs.microsoft.com/en-us/windows/win32/api/audioclient/nf-audioclient-iaudioclient-initialize
                    match werr.code() {
                        E_INVALIDARG => error!("IAudioClient::Initialize: Invalid argument"),
                        AUDCLNT_E_BUFFER_SIZE_NOT_ALIGNED => {
                            debug!("IAudioClient::Initialize: Unaligned buffer, trying to adjust the period.");
                            // Try to recover following the example in the docs.
                            // https://learn.microsoft.com/en-us/windows/win32/api/audioclient/nf-audioclient-iaudioclient-initialize#examples
                            // Just panic on errors to keep it short and simple.
                            // 1. Call IAudioClient::GetBufferSize and receive the next-highest-aligned buffer size (in frames).
                            let buffersize = client.get_bufferframecount().map_err(|e| {
                                anyhow!("IAudioClient::GetBufferSize failed: {}", e)
                            })?;
                            debug!(
                                "Client next-highest-aligned buffer size: {} frames",
                                buffersize
                            );
                            // 2. Call IAudioClient::Release, skipped since this will happen automatically when we drop the client.
                            // 3. Calculate the aligned buffer size in 100-nanosecond units.
                            let aligned_period = calculate_period_100ns(
                                buffersize as i64,
                                wave_format.get_samplespersec() as i64,
                            );
                            debug!("Aligned period in 100ns units: {}", aligned_period);
                            // 4. Get a new IAudioClient
                            client = device.inner_device.get_iaudioclient().map_err(|e| {
                                anyhow!("IAudioClient::GetAudioClient failed: {}", e)
                            })?;
                            // 5. Call Initialize again on the created audio client.
                            client
                                .initialize_client(
                                    &wave_format,
                                    aligned_period as i64,
                                    &Direction::Render,
                                    &ShareMode::Exclusive,
                                    false,
                                )
                                .map_err(|e| anyhow!("IAudioClient::Initialize failed: {}", e))?;
                            debug!("IAudioClient::Initialize ok");
                        }
                        AUDCLNT_E_DEVICE_IN_USE => {
                            error!("IAudioClient::Initialize: The device is already in use");
                            panic!("IAudioClient::Initialize failed");
                        }
                        AUDCLNT_E_UNSUPPORTED_FORMAT => {
                            error!("IAudioClient::Initialize The device does not support the audio format");
                            panic!("IAudioClient::Initialize failed");
                        }
                        AUDCLNT_E_EXCLUSIVE_MODE_NOT_ALLOWED => {
                            error!("IAudioClient::Initialize: Exclusive mode is not allowed");
                            panic!("IAudioClient::Initialize failed");
                        }
                        AUDCLNT_E_ENDPOINT_CREATE_FAILED => {
                            error!("IAudioClient::Initialize: Failed to create endpoint");
                            panic!("IAudioClient::Initialize failed");
                        }
                        _ => {
                            error!("IAudioClient::Initialize: Other error, HRESULT: {:#010x}, info: {:?}", werr.code().0, werr.message());
                            panic!("IAudioClient::Initialize failed");
                        }
                    };
                } else {
                    error!("IAudioClient::Initialize: Other error {:?}", e);
                }
            }
        };

        let eventhandle = client
            .set_get_eventhandle()
            .map_err(|e| anyhow!("IAudioClient::SetEventHandle failed: {}", e))?;
        let renderer = client
            .get_audiorenderclient()
            .map_err(|e| anyhow!("IAudioClient::GetAudioRenderClient failed: {}", e))?;
        Ok(Streamer {
            client,
            renderer,
            wave_format,
            eventhandle,
            pause_condition: Condvar::new(),
            status: Mutex::new(StreamingCommand::None),
            receiver,
        })
    }
    pub(super) fn wait_readiness(&self) {
        let status = self.status.lock().expect("fail to lock status mutex");
        let _ = self.pause_condition.wait(status);
    }

    pub(crate) fn start(&mut self) -> Result<()> {
        self.client
            .start_stream()
            .map_err(|e| anyhow!("IAudioClient::StartStream failed: {:?}", e))?;
        let mut buffer = vec![];
        loop {
            if let Ok(command) = self.receiver.recv() {
                match command {
                    StreamingCommand::Data(data) => {
                        let available_frames =
                            self.client.get_available_space_in_frames().map_err(|e| {
                                anyhow!("IAudioClient::GetAvailableSpaceInFrames failed: {:?}", e)
                            })?;
                        let available_buffer_len =
                            available_frames as usize * self.wave_format.get_blockalign() as usize;

                        buffer.push(data);
                        if buffer.len() != available_buffer_len {
                            continue;
                        }

                        self.renderer
                            .write_to_device(
                                available_frames as usize,
                                self.wave_format.get_blockalign() as usize,
                                buffer.as_mut_slice(),
                                None,
                            )
                            .map_err(|e| anyhow!("IAudioRenderClient::Write failed: {:?}", e))?;

                        buffer.clear();
                        self.eventhandle
                            .wait_for_event(1000)
                            .map_err(|e| anyhow!("WaitForSingleObject failed: {:?}", e))?;
                    }
                    StreamingCommand::Pause => {
                        *self.status.lock().unwrap() = command;
                        self.client
                            .stop_stream()
                            .map_err(|e| anyhow!("IAudioClient::StopStream failed: {:?}", e))?;
                        self.wait_readiness();
                        self.client
                            .start_stream()
                            .map_err(|e| anyhow!("IAudioClient::StartStream failed: {:?}", e))?;
                    }
                    StreamingCommand::Stop => {
                        return Ok(());
                    }
                    _ => {}
                }
            } else {
                //self.client
                //    .reset_stream()
                //    .map_err(|e| anyhow!("IAudioClient::StartStream failed: {:?}", e))?;
                return self
                    .client
                    .stop_stream()
                    .map_err(|e| anyhow!("IAudioClient::StopStream failed: {:?}", e));
            }
        }
    }
}
