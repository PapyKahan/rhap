//
// reference : Shared mode streaming : https://learn.microsoft.com/en-us/windows/win32/coreaudio/rendering-a-stream
// reference : Exclusive mode streaming : https://learn.microsoft.com/en-us/windows/win32/coreaudio/exclusive-mode-streams
// reference : https://www.hresult.info/FACILITY_AUDCLNT
//
use std::mem::size_of;
use windows::core::PCWSTR;
use windows::s;
use windows::Win32::Foundation::{
    CloseHandle, FALSE, HANDLE, S_OK, WAIT_FAILED, WAIT_OBJECT_0, WAIT_TIMEOUT,
};
use windows::Win32::Media::Audio::{
    IAudioClient, IAudioRenderClient, 
    AUDCLNT_E_BUFFER_SIZE_NOT_ALIGNED, AUDCLNT_SHAREMODE_EXCLUSIVE, AUDCLNT_SHAREMODE_SHARED,
    AUDCLNT_STREAMFLAGS_EVENTCALLBACK, WAVEFORMATEX, WAVEFORMATEXTENSIBLE, WAVEFORMATEXTENSIBLE_0, IMMDevice,
};
use windows::Win32::Media::KernelStreaming::{
    KSDATAFORMAT_SUBTYPE_PCM, SPEAKER_FRONT_LEFT, SPEAKER_FRONT_RIGHT, WAVE_FORMAT_EXTENSIBLE,
};
use windows::Win32::System::Com::{
    CoInitializeEx, CLSCTX_ALL, COINIT_MULTITHREADED,
};
use windows::Win32::System::Threading::{
    AvRevertMmThreadCharacteristics, AvSetMmThreadCharacteristicsA, CreateEventW,
    WaitForSingleObject,
};

use super::utils::{host_error, align_frames_per_buffer, align_bwd};
use crate::audio::api::wasapi::utils::{make_hns_period, make_frames_from_hns};
use crate::audio::{StreamFlow, StreamParams, StreamTrait};

pub struct Stream {
    params: StreamParams,
    client: IAudioClient,
    renderer: IAudioRenderClient,
    buffersize: u32,
    eventhandle: HANDLE,
    threadhandle: HANDLE,
    callback: Box<dyn FnMut(&mut [u8], usize) -> Result<StreamFlow, String> + Send + 'static>,
}

impl Stream {
    // WAVEFORMATEX documentation: https://learn.microsoft.com/en-us/windows/win32/api/mmreg/ns-mmreg-waveformatex
    // WAVEFORMATEXTENSIBLE documentation: https://docs.microsoft.com/en-us/windows/win32/api/mmreg/ns-mmreg-waveformatextensible
    #[inline(always)]
    pub(crate) fn create_waveformat_from(params: StreamParams) -> WAVEFORMATEXTENSIBLE {
        let formattag = WAVE_FORMAT_EXTENSIBLE;
        let channels = params.channels as u32;
        let sample_rate: u32 = params.samplerate as u32;
        let bits_per_sample: u32 = params.bits_per_sample as u32;
        let block_align: u32 = channels * bits_per_sample / 8;
        let bytes_per_second = sample_rate * block_align;

        WAVEFORMATEXTENSIBLE {
            Format: WAVEFORMATEX {
                wFormatTag: formattag as u16,
                nChannels: channels as u16,
                nSamplesPerSec: sample_rate,
                wBitsPerSample: bits_per_sample as u16,
                nBlockAlign: block_align as u16,
                nAvgBytesPerSec: bytes_per_second,
                cbSize: size_of::<WAVEFORMATEXTENSIBLE>() as u16 - size_of::<WAVEFORMATEX>() as u16,
            },
            Samples: WAVEFORMATEXTENSIBLE_0 {
                wValidBitsPerSample: bits_per_sample as u16,
            },
            dwChannelMask: SPEAKER_FRONT_LEFT | SPEAKER_FRONT_RIGHT,
            SubFormat: KSDATAFORMAT_SUBTYPE_PCM,
        }
    }

    pub(super) fn build_from_device<T>(
        device: &IMMDevice,
        params: StreamParams,
        callback: T,
    ) -> Result<Stream, String>
    where
        T: FnMut(&mut [u8], usize) -> Result<StreamFlow, String> + Send + 'static,
    {
        unsafe {
            match CoInitializeEx(None, COINIT_MULTITHREADED) {
                Ok(_) => (),
                Err(err) => {
                    return Err(format!("Error initializing COM: {} - {}", err.code(), err));
                }
            };

            let client: IAudioClient = match (*device).Activate::<IAudioClient>(CLSCTX_ALL, None)
            {
                Ok(client) => client,
                Err(err) => {
                    return Err(format!(
                        "Error activating device: {} - {}",
                        host_error(err.code()),
                        err
                    ));
                }
            };

            let wave_format = Stream::create_waveformat_from(params.clone());
            let sharemode = match params.exclusive {
                true => AUDCLNT_SHAREMODE_EXCLUSIVE,
                false => AUDCLNT_SHAREMODE_SHARED,
            };

            let streamflags = AUDCLNT_STREAMFLAGS_EVENTCALLBACK;
            match client.IsFormatSupported(
                sharemode,
                &wave_format.Format as *const WAVEFORMATEX,
                None,
            ) {
                S_OK => true,
                result => {
                    return Err(format!(
                        "Error checking format support: {} - {}",
                        host_error(result),
                        "Unsuported format"
                    ));
                }
            };

            let mut default_device_period: i64 = 0;
            let mut minimum_device_period: i64 = 0;
            if params.buffer_length == 0 {
                match client.GetDevicePeriod(
                    Some(&mut default_device_period as *mut i64),
                    Some(&mut minimum_device_period as *mut i64),
                ) {
                    Ok(_) => (),
                    Err(err) => {
                        return Err(format!(
                            "Error getting device period: {} - {}",
                            host_error(err.code()),
                            err
                        ));
                    }
                };
            } else {
                default_device_period = (params.buffer_length * 1000000) / 100 as i64;
            }

            let result = client.Initialize(
                sharemode,
                streamflags,
                default_device_period,
                default_device_period,
                &wave_format.Format as *const WAVEFORMATEX,
                Some(std::ptr::null()),
            );

            if result.is_err() {
                if result.as_ref().err().unwrap().code() != AUDCLNT_E_BUFFER_SIZE_NOT_ALIGNED {
                    let err = result.err().unwrap();
                    return Err(format!(
                        "Error initializing client: {} - {}",
                        host_error(err.code()),
                        err
                    ));
                }
                println!("Buffer size not aligned");
                let buffer_size = match client.GetBufferSize() {
                    Ok(buffer_size) => buffer_size as i64,
                    Err(err) => {
                        return Err(format!("Initialize: Error getting buffer size: {}", err));
                    }
                };
                let frames_per_latency = make_frames_from_hns(default_device_period as u32, wave_format.Format.nSamplesPerSec);
                let frames_per_latency = align_frames_per_buffer(frames_per_latency, wave_format.Format.nBlockAlign as u32, align_bwd);
                let period = make_hns_period(frames_per_latency, wave_format.Format.nSamplesPerSec);

                let period = if buffer_size as u32 >= (frames_per_latency * 2) {
                    let ratio = buffer_size as u32 / frames_per_latency;
                    let frames_per_latency = make_hns_period(period / ratio, wave_format.Format.nSamplesPerSec);
                    let frames_per_latency = align_frames_per_buffer(frames_per_latency, wave_format.Format.nBlockAlign as u32, align_bwd);
                    let period = make_hns_period(frames_per_latency, wave_format.Format.nSamplesPerSec);
                    if period < minimum_device_period as u32 {
                        minimum_device_period as u32
                    } else {
                        period as u32
                    }
                } else {
                    period
                };

                match client.Initialize(
                    sharemode,
                    streamflags,
                    period as i64,
                    period as i64,
                    &wave_format.Format as *const WAVEFORMATEX,
                    Some(std::ptr::null()),
                ) {
                    Ok(_) => (),
                    Err(err) => {
                        return Err(format!("Error initializing client: {}", err));
                    }
                }
            }

            let eventhandle = match CreateEventW(None, FALSE, FALSE, PCWSTR::null()) {
                Ok(eventhandle) => eventhandle,
                Err(err) => {
                    return Err(format!(
                        "Error creating event handle: {} - {}",
                        host_error(err.code()),
                        err
                    ));
                }
            };

            match client.SetEventHandle(eventhandle) {
                Ok(_) => (),
                Err(err) => {
                    return Err(format!(
                        "Error setting event handle: {} - {}",
                        host_error(err.code()),
                        err
                    ));
                }
            }

            let buffersize = match client.GetBufferSize() {
                Ok(buffer_size) => buffer_size,
                Err(err) => {
                    return Err(format!(
                        "Size: Error getting buffer size: {} - {}",
                        host_error(err.code()),
                        err
                    ));
                }
            };

            let renderer: IAudioRenderClient = match client.GetService::<IAudioRenderClient>() {
                Ok(client_renderer) => client_renderer,
                Err(err) => {
                    return Err(format!(
                        "Error getting client renderer: {} - {}",
                        host_error(err.code()),
                        err
                    ));
                }
            };
            Ok(Stream {
                params,
                client,
                renderer,
                buffersize,
                threadhandle: HANDLE::default(),
                eventhandle,
                callback: Box::new(callback),
            })
        }
    }
}

impl StreamTrait for Stream {
    fn start(&mut self) -> Result<(), String> {
        println!("Starting stream with parameters: {:?}", self.params);
        unsafe {
            let mut task_index: u32 = 0;
            let task_index: *mut u32 = &mut task_index;
            self.threadhandle = match AvSetMmThreadCharacteristicsA(s!("Pro Audio"), task_index) {
                Ok(handle) => handle,
                Err(error) => {
                    return Err(format!(
                        "Error setting thread characteristics: {} - {}",
                        host_error(error.code()),
                        error
                    ));
                }
            };

            match self.client.Start() {
                Ok(_) => (),
                Err(err) => {
                    return Err(format!(
                        "Error starting client: {} - {}",
                        host_error(err.code()),
                        err
                    ));
                }
            }

            loop {
                match WaitForSingleObject(self.eventhandle, 2000) {
                    WAIT_OBJECT_0 => (),
                    WAIT_TIMEOUT => {
                        println!("Timeout");
                        break;
                    }
                    WAIT_FAILED => {
                        println!("Wait failed");
                        break;
                    }
                    _ => (),
                }

                let client_buffer = match self.renderer.GetBuffer(self.buffersize) {
                    Ok(buffer) => buffer,
                    Err(err) => {
                        return Err(format!("Error getting client buffer: {}", err));
                    }
                };

                // Convert client buffer to a slice of bytes.
                let client_buffer_len = self.buffersize as usize
                    * (self.params.bits_per_sample as usize / 8) as usize
                    * self.params.channels as usize;
                let data = std::slice::from_raw_parts_mut(client_buffer, client_buffer_len);
                let result = match (self.callback)(data, client_buffer_len) {
                    Ok(result) => result,
                    Err(err) => {
                        return Err(format!("Error calling callback: {}", err));
                    }
                };

                match result {
                    StreamFlow::Complete => {
                        break;
                    }
                    StreamFlow::Abort => {
                        break;
                    }
                    StreamFlow::Continue => (),
                };

                match self.renderer.ReleaseBuffer(self.buffersize, 0) {
                    Ok(_) => (),
                    Err(err) => {
                        return Err(format!(
                            "Error releasing client buffer: {} - {}",
                            host_error(err.code()),
                            err
                        ));
                    }
                };
            }
        }
        Ok(())
    }

    fn stop(&self) -> Result<(), String> {
        println!("Stopping stream with parameters: {:?}", self.params);
        unsafe {
            match self.client.Stop() {
                Ok(_) => (),
                Err(err) => {
                    return Err(format!(
                        "Error stopping client: {} - {}",
                        host_error(err.code()),
                        err
                    ));
                }
            };
            AvRevertMmThreadCharacteristics(self.threadhandle);
            CloseHandle(self.eventhandle);
        }
        Ok(())
    }

    fn pause(&self) -> Result<(), String> {
        println!("Pausing stream with parameters: {:?}", self.params);
        Ok(())
    }

    fn resume(&self) -> Result<(), String> {
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
