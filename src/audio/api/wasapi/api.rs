use anyhow::{anyhow, Result};
use log::debug;
use log::error;
use num_integer::Integer;
use std::cmp;
use windows::core::w;
use windows::Win32::Foundation::E_INVALIDARG;
use windows::Win32::Media::Audio::IMMDevice;
use windows::Win32::Media::Audio::AUDCLNT_E_BUFFER_SIZE_NOT_ALIGNED;
use windows::Win32::Media::Audio::AUDCLNT_E_DEVICE_IN_USE;
use windows::Win32::Media::Audio::AUDCLNT_E_ENDPOINT_CREATE_FAILED;
use windows::Win32::Media::Audio::AUDCLNT_E_EXCLUSIVE_MODE_NOT_ALLOWED;
use windows::Win32::Media::Audio::AUDCLNT_E_UNSUPPORTED_FORMAT;
use windows::Win32::System::Com::CLSCTX_ALL;
use windows::Win32::System::Threading::AvRevertMmThreadCharacteristics;
use windows::Win32::System::Threading::AvSetMmThreadCharacteristicsW;
use windows::Win32::System::Threading::GetCurrentProcess;
use windows::Win32::System::Threading::GetCurrentThread;
use windows::Win32::System::Threading::GetPriorityClass;
use windows::Win32::System::Threading::GetThreadPriority;
use windows::Win32::System::Threading::SetPriorityClass;
use windows::Win32::System::Threading::SetThreadPriority;
use windows::Win32::System::Threading::HIGH_PRIORITY_CLASS;
use windows::Win32::System::Threading::PROCESS_CREATION_FLAGS;
use windows::Win32::System::Threading::THREAD_PRIORITY;
use windows::Win32::System::Threading::THREAD_PRIORITY_HIGHEST;
use windows::{
    core::PCSTR,
    Win32::{
        Foundation::{HANDLE, RPC_E_CHANGED_MODE, WAIT_OBJECT_0},
        Media::{
            Audio::{
                IAudioClient, IAudioRenderClient, AUDCLNT_SHAREMODE_EXCLUSIVE,
                AUDCLNT_SHAREMODE_SHARED, AUDCLNT_STREAMFLAGS_EVENTCALLBACK, WAVEFORMATEX,
                WAVEFORMATEXTENSIBLE, WAVEFORMATEXTENSIBLE_0,
            },
            KernelStreaming::{KSDATAFORMAT_SUBTYPE_PCM, WAVE_FORMAT_EXTENSIBLE},
            Multimedia::KSDATAFORMAT_SUBTYPE_IEEE_FLOAT,
        },
        System::{
            Com::{CoInitializeEx, CoUninitialize, COINIT_MULTITHREADED},
            Threading::{CreateEventA, WaitForSingleObject},
        },
    },
};

use crate::audio::{BitsPerSample, StreamParams};

thread_local! {
    static WASAPI_COM_INIT: ComWasapi = {
        let result = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
        if result.0 < 0 {
            if result == RPC_E_CHANGED_MODE {
                ComWasapi { is_ok: true }
            } else {
                panic!("Failed to initialize COM: HRESULT {}", result);
            }
        } else {
            ComWasapi { is_ok: true }
        }
    }
}

struct ComWasapi {
    is_ok: bool,
}

impl Drop for ComWasapi {
    #[inline]
    fn drop(&mut self) {
        if self.is_ok {
            unsafe { CoUninitialize() }
        }
    }
}

#[inline]
pub fn com_initialize() {
    WASAPI_COM_INIT.with(|_| {})
}

pub fn calculate_period_100ns(frames: i64, samplerate: i64) -> i64 {
    ((10000.0 * 1000.0 / samplerate as f64 * frames as f64) + 0.5) as i64
}

#[derive(Clone)]
pub enum ShareMode {
    Exclusive,
    Shared,
}

pub struct AudioClient {
    inner_client: IAudioClient,
    format: WaveFormat,
    renderer: Option<AudioRenderClient>,
    period: Option<i64>,
    max_buffer_frames: usize,
    sharemode: ShareMode,
}

impl Drop for AudioClient {
    fn drop(&mut self) {
        unsafe {
            let _ = self.inner_client.Stop();
            let _ = self.inner_client.Reset();
        }
    }
}

impl AudioClient {
    pub fn is_supported(&self, format: WaveFormat, share_mode: &ShareMode) -> Result<WaveFormat> {
        match share_mode {
            ShareMode::Exclusive => self.is_supported_exclusive(format),
            ShareMode::Shared => self.is_supported_shared(format),
        }
    }

    pub fn write(&self, data: &[u8]) -> Result<()> {
        if let Some(renderer) = &self.renderer {
            let frames = data.len() / self.format.get_block_align() as usize;
            renderer.write(frames, self.format.get_block_align() as usize, data, None)?;
        }
        Ok(())
    }

    pub fn get_period(&self) -> i64 {
        self.period.unwrap_or(0)
    }

    fn is_supported_exclusive(&self, format: WaveFormat) -> Result<WaveFormat> {
        let first_test = unsafe {
            self.inner_client
                .IsFormatSupported(AUDCLNT_SHAREMODE_EXCLUSIVE, &format.0.Format, None)
                .ok()
        };
        if first_test.is_ok() {
            return Ok(format);
        }
        //perform a second test with WAVEFORMATEX if channel mask is less than 2
        if format.0.dwChannelMask <= 2 {
            let wave_format = format.0.Format.clone();
            unsafe {
                self.inner_client
                    .IsFormatSupported(AUDCLNT_SHAREMODE_EXCLUSIVE, &wave_format, None)
                    .ok()?
            };
            return Ok(format);
        }
        Err(anyhow!("Format not supported"))
    }

    fn is_supported_shared(&self, format: WaveFormat) -> Result<WaveFormat> {
        let mut closest_match: *mut WAVEFORMATEX = std::ptr::null_mut();
        let result = unsafe {
            self.inner_client
                .IsFormatSupported(
                    AUDCLNT_SHAREMODE_SHARED,
                    &format.0.Format,
                    Some(&mut closest_match),
                )
                .ok()
        };
        if result.is_ok() {
            return Ok(format);
        } else {
            let fmt: WAVEFORMATEX = unsafe { closest_match.read() };
            Ok(WaveFormat::from_waveformatex(fmt)?)
        }
    }

    pub fn get_default_and_min_periods(&self) -> Result<(i64, i64)> {
        let mut default_period = 0;
        let mut min_period = 0;
        unsafe {
            self.inner_client
                .GetDevicePeriod(Some(&mut default_period), Some(&mut min_period))?
        };
        Ok((default_period, min_period))
    }

    pub fn calculate_aligned_period_near(
        &self,
        desired_period: i64,
        align_bytes: Option<u32>,
    ) -> Result<i64> {
        let (_, min_period) = self.get_default_and_min_periods()?;
        let adjusted_period = cmp::max(desired_period, min_period);
        let frame_bytes = self.format.get_block_align() as u32;
        let period_alignment_bytes = match align_bytes {
            Some(0) => frame_bytes,
            Some(bytes) => frame_bytes.lcm(&bytes),
            None => frame_bytes,
        };
        let period_alignment_frames = period_alignment_bytes as i64 / frame_bytes as i64;
        let desired_period_frames =
            (adjusted_period as f64 * self.format.0.Format.nSamplesPerSec as f64 / 10000000.0).round()
                as i64;
        let min_period_frames = (min_period as f64 * self.format.0.Format.nSamplesPerSec as f64
            / 10000000.0)
            .ceil() as i64;
        let mut nbr_segments = desired_period_frames / period_alignment_frames;
        if nbr_segments * period_alignment_frames < min_period_frames {
            // Add one segment if the value got rounded down below the minimum
            nbr_segments += 1;
        }
        let aligned_period = calculate_period_100ns(
            period_alignment_frames * nbr_segments,
            self.format.0.Format.nSamplesPerSec as i64,
        );
        Ok(aligned_period)
    }

    pub(crate) fn initialize(&mut self) -> Result<()> {
        let mode = match self.sharemode {
            ShareMode::Exclusive => AUDCLNT_SHAREMODE_EXCLUSIVE,
            ShareMode::Shared => AUDCLNT_SHAREMODE_SHARED,
        };

        let (_, min_device_period) = self.get_default_and_min_periods()?;
        // Calculate desired period for better device compatibility.
        let mut desired_period =
            self.calculate_aligned_period_near(3 * min_device_period / 2, Some(128))?;
        let device_period = match self.sharemode {
            ShareMode::Exclusive => desired_period,
            ShareMode::Shared => 0,
        };
        let flags = match self.sharemode {
            ShareMode::Exclusive => 0,
            ShareMode::Shared => AUDCLNT_STREAMFLAGS_EVENTCALLBACK,
        };

        unsafe {
            let result = self.inner_client.Initialize(
                mode,
                flags,
                desired_period,
                device_period,
                self.format.get_format(),
                None,
            );
            self.max_buffer_frames = self.inner_client.GetBufferSize()? as usize;
            match result {
                Ok(()) => debug!("IAudioClient::Initialize ok"),
                Err(e) => {
                    // Some of the possible errors. See the documentation for the full list and descriptions.
                    // https://docs.microsoft.com/en-us/windows/win32/api/audioclient/nf-audioclient-iaudioclient-initialize
                    match e.code() {
                        E_INVALIDARG => error!("IAudioClient::Initialize: Invalid argument"),
                        AUDCLNT_E_BUFFER_SIZE_NOT_ALIGNED => {
                            debug!("IAudioClient::Initialize: Unaligned buffer, trying to adjust the period.");
                            // Try to recover following the example in the docs.
                            // https://learn.microsoft.com/en-us/windows/win32/api/audioclient/nf-audioclient-iaudioclient-initialize#examples
                            // Just panic on errors to keep it short and simple.
                            // 1. Call IAudioClient::GetBufferSize and receive the next-highest-aligned buffer size (in frames).
                            debug!(
                                "Client next-highest-aligned buffer size: {} frames",
                                self.max_buffer_frames
                            );
                            // 2. Call IAudioClient::Release, skipped since this will happen automatically when we drop the client.
                            // 3. Calculate the aligned buffer size in 100-nanosecond units.
                            desired_period = calculate_period_100ns(
                                self.max_buffer_frames as i64,
                                self.format.get_samples_per_sec() as i64,
                            );
                            debug!("Aligned period in 100ns units: {}", desired_period);
                            // 4. Get a new IAudioClient
                            //self.inner_client = self.inner_client.Cast()?;
                            // 5. Call Initialize again on the created audio client.
                            //self.initialize()?;
                            self.inner_client.Initialize(
                                mode,
                                flags,
                                desired_period,
                                device_period,
                                self.format.get_format(),
                                None,
                            )?;
                            debug!("IAudioClient::Initialize ok");
                        }
                        AUDCLNT_E_DEVICE_IN_USE => {
                            error!("IAudioClient::Initialize: The device is already in use");
                            panic!("IAudioClient::Initialize: The device is already in use");
                        }
                        AUDCLNT_E_UNSUPPORTED_FORMAT => {
                            error!("IAudioClient::Initialize The device does not support the audio format");
                            panic!("IAudioClient::Initialize The device does not support the audio format");
                        }
                        AUDCLNT_E_EXCLUSIVE_MODE_NOT_ALLOWED => {
                            error!("IAudioClient::Initialize: Exclusive mode is not allowed");
                            panic!("IAudioClient::Initialize: Exclusive mode is not allowed");
                        }
                        AUDCLNT_E_ENDPOINT_CREATE_FAILED => {
                            error!("IAudioClient::Initialize: Failed to create endpoint");
                            panic!("IAudioClient::Initialize: Failed to create endpoint");
                        }
                        _ => {
                            error!("IAudioClient::Initialize: Other error, HRESULT: {:#010x}, info: {:?}", e.code().0, e.message());
                            panic!("IAudioClient::Initialize: Other error, HRESULT: {:#010x}, info: {:?}", e.code().0, e.message());
                        }
                    };
                }
            };
        };

        self.renderer = Some(self.get_renderer()?);

        Ok(())
    }

    pub(crate) fn get_renderer(&self) -> Result<AudioRenderClient> {
        Ok(AudioRenderClient(unsafe {
            self.inner_client.GetService::<IAudioRenderClient>()?
        }))
    }

    pub(crate) fn stop(&self) -> Result<()> {
        Ok(unsafe {
            self.inner_client.Stop()?;
            self.inner_client.Reset()?
        })
    }

    pub(crate) fn get_max_buffer_frames(&self) -> usize {
        self.max_buffer_frames
    }

    pub(crate) fn get_available_buffer_size(&self) -> Result<usize> {
        //let frames = match self.sharemode {
        //    Some(ShareMode::Exclusive) => {
        //        let buffer_frame_count = unsafe { self.inner_client.GetBufferSize()? as usize };
        //        buffer_frame_count
        //    }
        //    Some(ShareMode::Shared) => {
        //        let padding_count = unsafe { self.inner_client.GetCurrentPadding()? as usize };
        //        let buffer_frame_count = unsafe { self.inner_client.GetBufferSize()? as usize };
        //        buffer_frame_count - padding_count
        //    }
        //    _ => return Err(anyhow!("Client has not been initialized")),
        //};

        let padding_count = unsafe { self.inner_client.GetCurrentPadding()? as usize };
        //let buffer_frame_count = unsafe { self.inner_client.GetBufferSize()? as usize };
        let frames = self.max_buffer_frames - padding_count;
        let size = frames * self.format.get_block_align() as usize;
        Ok(size)
    }

    pub(crate) fn get_samples_per_sec(&self) -> u32 {
        self.format.get_samples_per_sec()
    }


    pub(crate) fn new(device: &IMMDevice, params: &StreamParams) -> Result<AudioClient> {
        com_initialize();
        let sharemode = match params.exclusive {
            true => ShareMode::Exclusive,
            false => ShareMode::Shared,
        };
        let inner_client = unsafe { device.Activate::<IAudioClient>(CLSCTX_ALL, None)? };
        Ok(AudioClient {
            inner_client,
            format: WaveFormat::from(params),
            period: None,
            renderer: None,
            sharemode,
            max_buffer_frames: 0,
        })
    }

    pub(crate) fn start(&self) -> Result<()> {
        unsafe { self.inner_client.Start()? };
        Ok(())
    }

    pub(crate) fn set_get_eventhandle(&self) -> Result<EventHandle> {
        let handle = unsafe { CreateEventA(None, false, false, PCSTR::null())? };
        unsafe { self.inner_client.SetEventHandle(handle)? };
        Ok(EventHandle(handle))
    }
}

pub struct EventHandle(HANDLE);
impl EventHandle {
    pub(crate) fn wait_for_event(&self, timeout: u32) -> Result<()> {
        let retval = unsafe { WaitForSingleObject(self.0, timeout) };
        if retval.0 != WAIT_OBJECT_0.0 {
            return Err(anyhow!("Wait timed out"));
        }
        Ok(())
    }
}

pub struct AudioRenderClient(IAudioRenderClient);
impl AudioRenderClient {
    #[inline(always)]
    pub(crate) fn write(
        &self,
        available_frames: usize,
        n_block_align: usize,
        data: &[u8],
        buffer_flags: Option<u32>,
    ) -> Result<()> {
        let nbr_bytes = available_frames * n_block_align;
        if nbr_bytes > data.len() {
            return Err(anyhow!(
                "Wrong length of data, got {}, expected {}",
                data.len(),
                nbr_bytes
            ));
        }
        let flags = match buffer_flags {
            Some(bflags) => bflags,
            None => 0,
        };
        unsafe {
            let buffer_ptr = self.0.GetBuffer(available_frames as u32)?;
            std::ptr::copy_nonoverlapping(data.as_ptr(), buffer_ptr, nbr_bytes);
            self.0.ReleaseBuffer(available_frames as u32, flags)?;
        }
        Ok(())
    }
}

/// Struct wrapping a [WAVEFORMATEXTENSIBLE](https://docs.microsoft.com/en-us/windows/win32/api/mmreg/ns-mmreg-waveformatextensible) format descriptor.
#[derive(Clone)]
pub struct WaveFormat(WAVEFORMATEXTENSIBLE);

impl WaveFormat {
    pub fn new(bits_per_sample: BitsPerSample, samplerate: usize, channels: usize) -> Self {
        let blockalign = channels * bits_per_sample as usize / 8;
        let byterate = samplerate * blockalign;

        let valid_bits_per_sample = match bits_per_sample {
            BitsPerSample::Bits8 => 8,
            BitsPerSample::Bits16 => 16,
            BitsPerSample::Bits24 => 24,
            BitsPerSample::Bits32 => 32,
        };

        let wave_format = WAVEFORMATEX {
            cbSize: 22,
            nAvgBytesPerSec: byterate as u32,
            nBlockAlign: blockalign as u16,
            nChannels: channels as u16,
            nSamplesPerSec: samplerate as u32,
            wBitsPerSample: bits_per_sample as u16,
            wFormatTag: WAVE_FORMAT_EXTENSIBLE as u16,
        };
        let sample = WAVEFORMATEXTENSIBLE_0 {
            wValidBitsPerSample: valid_bits_per_sample,
        };
        let subformat = match bits_per_sample {
            BitsPerSample::Bits8 => KSDATAFORMAT_SUBTYPE_PCM,
            BitsPerSample::Bits16 => KSDATAFORMAT_SUBTYPE_PCM,
            BitsPerSample::Bits24 => KSDATAFORMAT_SUBTYPE_PCM,
            BitsPerSample::Bits32 => KSDATAFORMAT_SUBTYPE_IEEE_FLOAT,
        };
        // https://docs.microsoft.com/en-us/windows/win32/api/mmreg/ns-mmreg-waveformatextensible
        let mask = match channels {
            ch if ch <= 18 => {
                // setting bit for each channel
                (1 << ch) - 1
            }
            _ => 0,
        };
        let wave_fmt = WAVEFORMATEXTENSIBLE {
            Format: wave_format,
            Samples: sample,
            SubFormat: subformat,
            dwChannelMask: mask,
        };
        WaveFormat(wave_fmt)
    }

    /// convert from [WAVEFORMATEX](https://docs.microsoft.com/en-us/previous-versions/dd757713(v=vs.85)) structure
    fn from_waveformatex(wavefmt: WAVEFORMATEX) -> Result<Self> {
        let bits_per_sample = BitsPerSample::from(wavefmt.wBitsPerSample as usize);
        let samplerate = wavefmt.nSamplesPerSec as usize;
        let channels = wavefmt.nChannels as usize;
        Ok(WaveFormat::new(bits_per_sample, samplerate, channels))
    }

    pub(crate) fn get_samples_per_sec(&self) -> u32 {
        self.0.Format.nSamplesPerSec
    }

    #[inline(always)]
    pub(crate) fn get_block_align(&self) -> u16 {
        self.0.Format.nBlockAlign
    }

    pub(crate) fn get_format(&self) -> &WAVEFORMATEX {
        &self.0.Format
    }
}

// WAVEFORMATEX documentation: https://learn.microsoft.com/en-us/windows/win32/api/mmreg/ns-mmreg-waveformatex
// WAVEFORMATEXTENSIBLE documentation: https://docs.microsoft.com/en-us/windows/win32/api/mmreg/ns-mmreg-waveformatextensible
impl From<&StreamParams> for WaveFormat {
    #[inline(always)]
    fn from(value: &StreamParams) -> Self {
        WaveFormat::new(
            value.bits_per_sample,
            value.samplerate as usize,
            value.channels as usize,
        )
    }
}

pub struct ThreadPriority {
    previous_process_priority: PROCESS_CREATION_FLAGS,
    previous_thread_priority: THREAD_PRIORITY,
    taskhandle: HANDLE,
}

impl ThreadPriority {
    pub fn new() -> Result<ThreadPriority> {
        let previous_process_priority =
            unsafe { PROCESS_CREATION_FLAGS(GetPriorityClass(GetCurrentProcess())) };
        unsafe { SetPriorityClass(GetCurrentProcess(), HIGH_PRIORITY_CLASS)? }
        let previous_thread_priority =
            unsafe { THREAD_PRIORITY(GetThreadPriority(GetCurrentThread())) };
        unsafe { SetThreadPriority(GetCurrentThread(), THREAD_PRIORITY_HIGHEST)? };
        let taskhandle = unsafe { AvSetMmThreadCharacteristicsW(w!("Pro Audio"), &mut 0)? };
        Ok(ThreadPriority {
            previous_process_priority,
            previous_thread_priority,
            taskhandle,
        })
    }

    fn revert_thread_priority(&mut self) -> Result<()> {
        unsafe {
            SetPriorityClass(GetCurrentProcess(), self.previous_process_priority)?;
            SetThreadPriority(GetCurrentThread(), self.previous_thread_priority)?;
            AvRevertMmThreadCharacteristics(self.taskhandle)?;
        }
        Ok(())
    }
}

impl Drop for ThreadPriority {
    fn drop(&mut self) {
        let _ = self.revert_thread_priority();
    }
}
