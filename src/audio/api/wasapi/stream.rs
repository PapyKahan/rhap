use std::collections::VecDeque;
// reference : Shared mode streaming : https://learn.microsoft.com/en-us/windows/win32/coreaudio/rendering-a-stream
// reference : Exclusive mode streaming : https://learn.microsoft.com/en-us/windows/win32/coreaudio/exclusive-mode-streams
// reference : https://www.hresult.info/FACILITY_AUDCLNT
//
use std::mem::size_of;
use wasapi::calculate_period_100ns;
use wasapi::AudioClient;
use wasapi::AudioRenderClient;
use wasapi::Direction;
use wasapi::Handle;
use wasapi::ShareMode;
use wasapi::WaveFormat;
use windows::core::s;
use windows::Win32::Foundation::{CloseHandle, HANDLE, WAIT_FAILED, WAIT_OBJECT_0, WAIT_TIMEOUT};
use windows::Win32::Media::Audio::AUDCLNT_E_BUFFER_SIZE_NOT_ALIGNED;
use windows::Win32::System::Threading::{
    AvRevertMmThreadCharacteristics, AvSetMmThreadCharacteristicsA, WaitForSingleObject,
};

use super::com::com_initialize;
use super::device::Device;
use super::utils::host_error;
use crate::audio::{StreamFlow, StreamParams, StreamTrait};

pub struct Stream {
    params: StreamParams,
    client: AudioClient,
    renderer: AudioRenderClient,
    buffersize: u32,
    eventhandle: Handle,
    wave_format: WaveFormat,
    threadhandle: Option<Handle>,
}

impl Stream {
    // WAVEFORMATEX documentation: https://learn.microsoft.com/en-us/windows/win32/api/mmreg/ns-mmreg-waveformatex
    // WAVEFORMATEXTENSIBLE documentation: https://docs.microsoft.com/en-us/windows/win32/api/mmreg/ns-mmreg-waveformatextensible
    #[inline(always)]
    pub(super) fn create_waveformat_from(params: StreamParams) -> WaveFormat {
        return WaveFormat::new(
            params.bits_per_sample as usize,
            params.bits_per_sample as usize,
            &wasapi::SampleType::Int,
            params.samplerate as usize,
            params.channels as usize,
            None,
        );
        //let formattag = WAVE_FORMAT_EXTENSIBLE;
        //let channels = params.channels as u32;
        //let sample_rate: u32 = params.samplerate as u32;
        //let bits_per_sample: u32 = params.bits_per_sample as u32;
        //let block_align: u32 = channels * bits_per_sample / 8;
        //let bytes_per_second = sample_rate * block_align;

        //WAVEFORMATEXTENSIBLE {
        //    Format: WAVEFORMATEX {
        //        wFormatTag: formattag as u16,
        //        nChannels: channels as u16,
        //        nSamplesPerSec: sample_rate,
        //        wBitsPerSample: bits_per_sample as u16,
        //        nBlockAlign: block_align as u16,
        //        nAvgBytesPerSec: bytes_per_second,
        //        cbSize: size_of::<WAVEFORMATEXTENSIBLE>() as u16 - size_of::<WAVEFORMATEX>() as u16,
        //    },
        //    Samples: WAVEFORMATEXTENSIBLE_0 {
        //        wValidBitsPerSample: bits_per_sample as u16,
        //    },
        //    dwChannelMask: SPEAKER_FRONT_LEFT | SPEAKER_FRONT_RIGHT,
        //    SubFormat: KSDATAFORMAT_SUBTYPE_PCM,
        //}
    }

    pub(super) fn build_from_device(
        device: &Device,
        params: StreamParams,
    ) -> Result<Stream, Box<dyn std::error::Error>> {
        com_initialize();
        let mut client = device.inner_device.get_iaudioclient()?;
        let wave_format = Stream::create_waveformat_from(params.clone());
        let sharemode = match params.exclusive {
            true => ShareMode::Exclusive,
            false => ShareMode::Shared,
        };

        let (mut default_device_period, minimum_device_period) = client.get_periods()?;
        if params.buffer_length != 0 {
            default_device_period = (params.buffer_length * 1000000) / 100 as i64;
        }

        let result = client.initialize_client(
            &wave_format,
            default_device_period,
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
        let buffersize = client.get_bufferframecount()?;
        let renderer = client.get_audiorenderclient()?;
        Ok(Stream {
            params,
            client,
            renderer,
            buffersize,
            wave_format,
            threadhandle: None,
            eventhandle,
        })
    }
}

impl StreamTrait for Stream {
    fn start(
        &mut self,
        callback: &mut dyn FnMut(
            &mut [u8],
            usize,
        ) -> Result<StreamFlow, Box<dyn std::error::Error>>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        println!("Starting stream with parameters: {:?}", self.params);
        self.client.start_stream()?;

        loop {
            let client_buffer_len = self.client.get_available_space_in_frames()?;
            let mut data = vec![0 as u8; client_buffer_len as usize];
            let data = data.as_mut_slice();
            let result = callback(data, client_buffer_len as usize)?;
            self.renderer.write_to_device(
                client_buffer_len as usize,
                self.wave_format.get_blockalign() as usize,
                data,
                None,
            );
            match result {
                StreamFlow::Complete => {
                    break;
                }
                StreamFlow::Abort => {
                    break;
                }
                StreamFlow::Continue => (),
            };
            if self.eventhandle.wait_for_event(1000).is_err() {
                println!("error, stopping playback");
                self.client.stop_stream()?;
                break;
            }
        }
        self.client.stop_stream()?;
        Ok(())
    }

    fn stop(&self) -> Result<(), Box<dyn std::error::Error>> {
        println!("Stopping stream with parameters: {:?}", self.params);
        self.client.stop_stream()
    }

    fn pause(&self) -> Result<(), Box<dyn std::error::Error>> {
        println!("Pausing stream with parameters: {:?}", self.params);
        Ok(())
    }

    fn resume(&self) -> Result<(), Box<dyn std::error::Error>> {
        println!("Resuming stream with parameters: {:?}", self.params);
        Ok(())
    }

    fn get_stream_params(&self) -> &StreamParams {
        &self.params
    }

    fn set_stream_params(&mut self, stream_paramters: StreamParams) {
        self.params = stream_paramters;
    }
}
