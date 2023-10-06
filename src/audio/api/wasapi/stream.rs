use std::collections::VecDeque;
use std::sync::Arc;
use std::sync::Mutex;
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
use crate::audio::{StreamParams, StreamTrait};

#[derive(Clone)]
pub struct Stream {
    params: StreamParams,
    audio_file_buffer: Arc<Mutex<VecDeque<u8>>>,
    client: Arc<AudioClient>,
    renderer: Arc<AudioRenderClient>,
    eventhandle: Arc<Handle>,
    wave_format: WaveFormat,
}

unsafe impl Send for Stream {}
unsafe impl Sync for Stream {}

impl Stream {
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

    pub(super) fn build_from_device(
        device: &Device,
        buffer: Arc<Mutex<VecDeque<u8>>>,
        params: StreamParams,
    ) -> Result<crate::audio::Stream, Box<dyn std::error::Error>> {
        com_initialize();
        let mut client = device.inner_device.get_iaudioclient()?;
        let wave_format = Stream::create_waveformat_from(params.clone());
        let sharemode = match params.exclusive {
            true => ShareMode::Exclusive,
            false => ShareMode::Shared,
        };

        let (default_device_period, _) = client.get_periods()?;
        let default_device_period = if params.buffer_length != 0 {
            (params.buffer_length * 1000000) / 100 as i64
        } else {
            default_device_period
        };

        // Calculatre desired period for better device compatibility. For our use case we don't
        // care about having a low lattency playback thats why we don't use minimum device period.
        let desired_period = client.calculate_aligned_period_near(
            3 * default_device_period / 2,
            Some(128),
            &wave_format,
        )?;
        let result = client.initialize_client(
            &wave_format,
            desired_period,
            &device.inner_device.get_direction(),
            &sharemode,
            false,
        );

        match result {
            Ok(()) => println!("IAudioClient::Initialize ok"),
            Err(e) => {
                if let Some(werr) = e.downcast_ref::<windows::core::Error>() {
                    // Some of the possible errors. See the documentation for the full list and descriptions.
                    // https://docs.microsoft.com/en-us/windows/win32/api/audioclient/nf-audioclient-iaudioclient-initialize
                    match werr.code() {
                        E_INVALIDARG => println!("IAudioClient::Initialize: Invalid argument"),
                        AUDCLNT_E_BUFFER_SIZE_NOT_ALIGNED => {
                            println!("IAudioClient::Initialize: Unaligned buffer, trying to adjust the period.");
                            // Try to recover following the example in the docs.
                            // https://learn.microsoft.com/en-us/windows/win32/api/audioclient/nf-audioclient-iaudioclient-initialize#examples
                            // Just panic on errors to keep it short and simple.
                            // 1. Call IAudioClient::GetBufferSize and receive the next-highest-aligned buffer size (in frames).
                            let buffersize = client.get_bufferframecount()?;
                            println!(
                                "Client next-highest-aligned buffer size: {} frames",
                                buffersize
                            );
                            // 2. Call IAudioClient::Release, skipped since this will happen automatically when we drop the client.
                            // 3. Calculate the aligned buffer size in 100-nanosecond units.
                            let aligned_period = calculate_period_100ns(
                                buffersize as i64,
                                wave_format.get_samplespersec() as i64,
                            );
                            println!("Aligned period in 100ns units: {}", aligned_period);
                            // 4. Get a new IAudioClient
                            client = device.inner_device.get_iaudioclient()?;
                            // 5. Call Initialize again on the created audio client.
                            client
                                .initialize_client(
                                    &wave_format,
                                    aligned_period as i64,
                                    &Direction::Render,
                                    &ShareMode::Exclusive,
                                    false,
                                )
                                .unwrap();
                            println!("IAudioClient::Initialize ok");
                        }
                        AUDCLNT_E_DEVICE_IN_USE => {
                            println!("IAudioClient::Initialize: The device is already in use");
                            panic!("IAudioClient::Initialize failed");
                        }
                        AUDCLNT_E_UNSUPPORTED_FORMAT => {
                            println!("IAudioClient::Initialize The device does not support the audio format");
                            panic!("IAudioClient::Initialize failed");
                        }
                        AUDCLNT_E_EXCLUSIVE_MODE_NOT_ALLOWED => {
                            println!("IAudioClient::Initialize: Exclusive mode is not allowed");
                            panic!("IAudioClient::Initialize failed");
                        }
                        AUDCLNT_E_ENDPOINT_CREATE_FAILED => {
                            println!("IAudioClient::Initialize: Failed to create endpoint");
                            panic!("IAudioClient::Initialize failed");
                        }
                        _ => {
                            println!("IAudioClient::Initialize: Other error, HRESULT: {:#010x}, info: {:?}", werr.code().0, werr.message());
                            panic!("IAudioClient::Initialize failed");
                        }
                    };
                } else {
                    panic!("IAudioClient::Initialize: Other error {:?}", e);
                }
            }
        };

        let eventhandle = client.set_get_eventhandle()?;
        let renderer = client.get_audiorenderclient()?;
        Ok(crate::audio::Stream::Wasapi(Stream {
            params,
            audio_file_buffer: buffer,
            client: Arc::new(client),
            renderer: Arc::new(renderer),
            wave_format,
            eventhandle: Arc::new(eventhandle),
        }))
    }
}

impl StreamTrait for Stream {
    fn start(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        println!("Starting stream with parameters: {:?}", self.params);

        self.client.start_stream()?;

        loop {
            let available_frames = self.client.get_available_space_in_frames()?;
            let available_buffer_len = available_frames as usize * self.wave_format.get_blockalign() as usize;

            let mut data = vec![0 as u8; available_buffer_len];
            for i in 0..available_buffer_len {
                if self.audio_file_buffer.lock().unwrap().is_empty() {
                    break;
                }
                data[i] = self.audio_file_buffer.lock().unwrap().pop_front().unwrap_or_default();
            }
            self.renderer.write_to_device(
                available_frames as usize,
                self.wave_format.get_blockalign() as usize,
                data.as_mut_slice(),
                None,
            )?;
            self.eventhandle.wait_for_event(1000)?;
            if self.audio_file_buffer.lock().unwrap().is_empty() {
                break;
            }
        }

        self.client.stop_stream()
    }

    fn stop(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        println!("Stopping stream with parameters: {:?}", self.params);
        Ok(())
    }

    fn pause(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        println!("Pausing stream with parameters: {:?}", self.params);
        Ok(())
    }

    fn resume(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        println!("Resuming stream with parameters: {:?}", self.params);
        Ok(())
    }

    fn get_stream_params(&self) -> StreamParams {
        self.params
    }

    fn set_stream_params(&mut self, parameters: StreamParams) {
        self.params = parameters;
    }
}
