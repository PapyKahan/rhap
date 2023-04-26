use std::mem::size_of;
use windows::core::{PCSTR, PCWSTR};
use windows::s;
use windows::Win32::Foundation::{
    CloseHandle, GetLastError, FALSE, HANDLE, S_OK, WAIT_FAILED, WAIT_OBJECT_0, WAIT_TIMEOUT,
};
use windows::Win32::Media::Audio::{
    IAudioClient, IAudioRenderClient, IMMDeviceEnumerator, MMDeviceEnumerator,
    AUDCLNT_E_BUFFER_SIZE_NOT_ALIGNED, AUDCLNT_SHAREMODE_EXCLUSIVE,
    AUDCLNT_STREAMFLAGS_EVENTCALLBACK, WAVEFORMATEX, WAVEFORMATEXTENSIBLE, WAVEFORMATEXTENSIBLE_0,
};
use windows::Win32::Media::KernelStreaming::{
    KSDATAFORMAT_SUBTYPE_PCM, SPEAKER_FRONT_LEFT, SPEAKER_FRONT_RIGHT, WAVE_FORMAT_EXTENSIBLE,
};
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CLSCTX_ALL, COINIT_MULTITHREADED,
};
use windows::Win32::System::Threading::{
    AvRevertMmThreadCharacteristics, AvSetMmThreadCharacteristicsA, CreateEventW,
    WaitForSingleObject,
};

use crate::audio::api::wasapi::utils::{host_error, print_wave_format};
use crate::audio::{DataProcessing, StreamParams, StreamTrait};

use super::enumerate_devices;

const REFTIMES_PER_SEC: i64 = 10000000;
//const REFTIMES_PER_MILLISEC : i64 = 10000;

fn _get_device(id: u16) -> Result<PCWSTR, String> {
    let mut selected_device: PCWSTR = PCWSTR(std::ptr::null_mut());

    let devices = match enumerate_devices() {
        Ok(devices) => devices,
        Err(err) => {
            println!("Error enumerating devices: {}", err);
            return Err(err);
        }
    };

    for dev in devices {
        if dev.index == id {
            selected_device = dev.id;
            break;
        }
    }

    if selected_device.is_null() {
        println!("Device not found");
        return Err("Device not found".to_string());
    }

    Ok(selected_device)
}

pub struct WasapiDevice {
    id: PCWSTR,
    pub index: u16,
    pub name: String,
}

impl WasapiDevice {
    pub fn new(inner_device_id: PCWSTR, index: u16, name: String) -> WasapiDevice {
        let this = Self {
            id: inner_device_id,
            index,
            name,
        };

        this
    }
}

pub struct WasapiStream {
    params: StreamParams,
    client: IAudioClient,
    renderer: IAudioRenderClient,
    buffersize: u32,
    eventhandle: HANDLE,
    callback: Box<dyn FnMut(&mut [u8], usize) -> Result<DataProcessing, String> + Send + 'static>,
}

impl StreamTrait for WasapiStream {
    fn new<T>(params: StreamParams, callback: T) -> Result<Self, String>
    where
        T: FnMut(&mut [u8], usize) -> Result<DataProcessing, String> + Send + 'static,
    {
        let selected_device = match _get_device(params.device.id) {
            Ok(device) => device,
            Err(err) => {
                return Err(format!("Error getting device: {}", err));
            }
        };

        unsafe {
            match CoInitializeEx(None, COINIT_MULTITHREADED) {
                Ok(_) => (),
                Err(err) => {
                    return Err(format!(
                        "Error initializing COM: {} - {}",
                        host_error(err.code()),
                        err
                    ));
                }
            };

            let enumerator: IMMDeviceEnumerator =
                match CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL) {
                    Ok(device_enumerator) => device_enumerator,
                    Err(err) => {
                        return Err(format!(
                            "Error getting device enumerator: {} - {}",
                            host_error(err.code()),
                            err
                        ));
                    }
                };

            let device = match enumerator.GetDevice(selected_device) {
                Ok(device) => device,
                Err(err) => {
                    return Err(format!(
                        "Error getting device: {} - {}",
                        host_error(err.code()),
                        err
                    ));
                }
            };

            // Crée un périphérique audio WASAPI exclusif.
            let client: IAudioClient = match device.Activate::<IAudioClient>(CLSCTX_ALL, None) {
                Ok(client) => client,
                Err(err) => {
                    return Err(format!(
                        "Error activating device: {} - {}",
                        host_error(err.code()),
                        err
                    ));
                }
            };

            //let wave_format = match client.GetMixFormat() {
            //    Ok(wave_format) => wave_format,
            //    Err(err) => {
            //        println!("Error getting mix format: {} - {}", audio::log::host_error(err.code()), err);
            //        return Err(());
            //    }
            //};

            let formattag = WAVE_FORMAT_EXTENSIBLE;
            let channels = params.channels as u32;
            let sample_rate: u32 = params.samplerate as u32;
            let bits_per_sample: u32 = params.bits_per_sample as u32;
            let block_align: u32 = channels * bits_per_sample / 8;
            let bytes_per_second = sample_rate * block_align;

            // WAVEFORMATEX documentation: https://learn.microsoft.com/en-us/windows/win32/api/mmreg/ns-mmreg-waveformatex
            // WAVEFORMATEXTENSIBLE documentation: https://docs.microsoft.com/en-us/windows/win32/api/mmreg/ns-mmreg-waveformatextensible
            let wave_format: *const WAVEFORMATEXTENSIBLE = &mut WAVEFORMATEXTENSIBLE {
                Format: WAVEFORMATEX {
                    wFormatTag: formattag as u16,
                    nChannels: channels as u16,
                    nSamplesPerSec: sample_rate,
                    wBitsPerSample: bits_per_sample as u16,
                    nBlockAlign: block_align as u16,
                    nAvgBytesPerSec: bytes_per_second,
                    cbSize: size_of::<WAVEFORMATEXTENSIBLE>() as u16
                        - size_of::<WAVEFORMATEX>() as u16,
                },
                Samples: WAVEFORMATEXTENSIBLE_0 {
                    wValidBitsPerSample: bits_per_sample as u16,
                },
                dwChannelMask: SPEAKER_FRONT_LEFT | SPEAKER_FRONT_RIGHT,
                SubFormat: KSDATAFORMAT_SUBTYPE_PCM,
            };

            println!("--------------------------------------------------------------------------------------");
            print_wave_format(wave_format as *const WAVEFORMATEX);
            println!("--------------------------------------------------------------------------------------");

            let sharemode = AUDCLNT_SHAREMODE_EXCLUSIVE;
            let streamflags = AUDCLNT_STREAMFLAGS_EVENTCALLBACK;
            match client.IsFormatSupported(sharemode, wave_format as *const WAVEFORMATEX, None) {
                S_OK => true,
                result => {
                    return Err(format!(
                        "Error checking format support: {} - {}",
                        host_error(result),
                        "Unsuporrted format"
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
                wave_format as *const WAVEFORMATEX,
                None,
            );

            println!("--------------------------------------------------------------------------------------");
            println!("Default device period: {}", default_device_period);
            println!("Minimum device period: {}", minimum_device_period);
            println!("--------------------------------------------------------------------------------------");

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
                let minimum_device_period = REFTIMES_PER_SEC / sample_rate as i64 * buffer_size;
                match client.Initialize(
                    sharemode,
                    streamflags,
                    minimum_device_period,
                    minimum_device_period,
                    wave_format as *const WAVEFORMATEX,
                    None,
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
            Ok(Self {
                params,
                client,
                renderer,
                buffersize,
                eventhandle,
                callback: Box::new(callback),
            })
        }
    }

    fn start(&mut self) -> Result<(), String> {
        println!("Starting stream with parameters: {:?}", self.params);
        // Compute client buffer size in bytes.
        let client_buffer_len = self.buffersize as usize
            * (self.params.bits_per_sample as usize / 8) as usize
            * self.params.channels as usize;
        unsafe {
            //let taskindex = std::ptr::null_mut();
            //let thread_handle = match AvSetMmThreadCharacteristicsA(s!("Pro Audio"), taskindex) {
            //    Ok(handle) => handle,
            //    Err(error) => {
            //        return Err(format!(
            //            "Error setting thread characteristics: {} - {}",
            //            host_error(error.code()),
            //            error
            //        ));
            //    }
            //};

            let client_buffer = match self.renderer.GetBuffer(self.buffersize) {
                Ok(buffer) => buffer,
                Err(err) => {
                    return Err(format!("Error getting client buffer: {}", err));
                }
            };

            // Convert client buffer to a slice of bytes.
            let data = std::slice::from_raw_parts_mut(client_buffer, client_buffer_len);
            match (self.callback)(data, client_buffer_len) {
                Ok(result) => result,
                Err(err) => {
                    return Err(format!("Error calling callback: {}", err));
                }
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
                let data = std::slice::from_raw_parts_mut(client_buffer, client_buffer_len);
                let result = match (self.callback)(data, client_buffer_len) {
                    Ok(result) => result,
                    Err(err) => {
                        return Err(format!("Error calling callback: {}", err));
                    }
                };

                match result {
                    DataProcessing::Complete => {
                        break;
                    }
                    DataProcessing::Abort => {
                        break;
                    }
                    DataProcessing::Continue => (),
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
            //AvRevertMmThreadCharacteristics(thread_handle);
            //CloseHandle(self.eventhandle);
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
