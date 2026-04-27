use anyhow::{anyhow, Context, Result};
use log::debug;
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

use crate::audio::{BitsPerSample, BufferConfig, SampleRate, StreamParams};

//const REFTIMES_PER_MILLISEC: u64 = 10000;
//const REFTIMES_PER_SEC: u64 = 10000000;

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
    fn drop(&mut self) {
        if self.is_ok {
            unsafe { CoUninitialize() }
        }
    }
}

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

/// Classified failure of `AudioClient::initialize`. The caller (Device::start)
/// dispatches retry policy: transient busy errors are retried with backoff,
/// alignment retries recreate the client with the suggested period, permanent
/// errors are surfaced immediately.
pub(crate) enum WasapiInitError {
    /// Transient: another stream holds the endpoint or the engine is still
    /// releasing it. Caller should sleep and retry with a fresh AudioClient.
    Busy,
    /// Buffer size not aligned. Caller should recreate the AudioClient and
    /// retry initialize with this period (in 100ns units), per MSDN.
    AlignmentRetry(i64),
    /// Permanent: the call cannot succeed by retrying as-is.
    Permanent(anyhow::Error),
}

pub struct AudioClient {
    inner_client: IAudioClient,
    format: WaveFormat,
    renderer: Option<AudioRenderClient>,
    max_buffer_frames: usize,
    sharemode: ShareMode,
    pollmode: bool,
    eventhandle: Option<EventHandle>,
}

// SAFETY: IAudioClient and IAudioRenderClient are COM pointers initialized in MTA
// (COINIT_MULTITHREADED). The audio output thread calls com_initialize() before
// using the client. AudioClient is moved into the audio thread, not shared.
unsafe impl Send for AudioClient {}
unsafe impl Sync for AudioClient {}

impl Drop for AudioClient {
    fn drop(&mut self) {
        let _ = self.stop();
    }
}

impl AudioClient {
    pub(crate) fn is_supported(
        &self,
        format: WaveFormat,
        share_mode: &ShareMode,
    ) -> Result<WaveFormat> {
        match share_mode {
            ShareMode::Exclusive => self.is_supported_exclusive(format),
            ShareMode::Shared => self.is_supported_shared(format),
        }
    }

    pub(crate) fn write(&self, data: &[u8]) -> Result<()> {
        if let Some(renderer) = &self.renderer {
            let frames = data.len() / self.format.get_block_align() as usize;
            renderer.write(frames, self.format.get_block_align() as usize, data, None)?;
        }
        Ok(())
    }

    /// Returns the number of bytes available to write in the WASAPI buffer.
    pub(crate) fn get_writable_size(&self) -> Result<usize> {
        self.get_available_buffer_frames()
            .map(|f| f * self.format.get_block_align() as usize)
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
            (adjusted_period as f64 * self.format.0.Format.nSamplesPerSec as f64 / 10000000.0)
                .round() as i64;
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

    /// Initialize the audio client with the given period (in 100ns units).
    /// Returns a classified error on failure so the caller can drive retry
    /// policy. On success, `self` is fully configured: `max_buffer_frames`,
    /// `renderer` and (in event mode) `eventhandle` are populated.
    pub(crate) fn initialize_with_period(
        &mut self,
        period_100ns: i64,
    ) -> std::result::Result<(), WasapiInitError> {
        let mode = match self.sharemode {
            ShareMode::Exclusive => AUDCLNT_SHAREMODE_EXCLUSIVE,
            ShareMode::Shared => AUDCLNT_SHAREMODE_SHARED,
        };
        let device_period = match self.sharemode {
            ShareMode::Exclusive => period_100ns,
            ShareMode::Shared => 0,
        };
        let flags = match self.sharemode {
            ShareMode::Exclusive | ShareMode::Shared => {
                if self.pollmode {
                    0
                } else {
                    AUDCLNT_STREAMFLAGS_EVENTCALLBACK
                }
            }
        };

        let result = unsafe {
            self.inner_client.Initialize(
                mode,
                flags,
                period_100ns,
                device_period,
                self.format.get_format(),
                None,
            )
        };

        match result {
            Ok(()) => {
                self.max_buffer_frames = unsafe { self.inner_client.GetBufferSize() }
                    .context("wasapi: get_buffer_size")
                    .map_err(WasapiInitError::Permanent)?
                    as usize;
                debug!("IAudioClient::Initialize ok");
            }
            Err(e) => {
                // https://learn.microsoft.com/en-us/windows/win32/api/audioclient/nf-audioclient-iaudioclient-initialize
                match e.code() {
                    AUDCLNT_E_BUFFER_SIZE_NOT_ALIGNED => {
                        // MSDN: query GetBufferSize, recompute aligned period,
                        // recreate the IAudioClient, retry. We surface the new
                        // period and let the caller recreate.
                        let aligned_frames = unsafe { self.inner_client.GetBufferSize() }
                            .map_err(|err| {
                                WasapiInitError::Permanent(anyhow!(
                                    "wasapi: get_buffer_size after BUFFER_SIZE_NOT_ALIGNED: {}",
                                    err
                                ))
                            })?
                            as i64;
                        let new_period = calculate_period_100ns(
                            aligned_frames,
                            self.format.get_samples_per_sec() as i64,
                        );
                        debug!(
                            "wasapi: aligned retry — frames={}, period={}00ns",
                            aligned_frames, new_period
                        );
                        return Err(WasapiInitError::AlignmentRetry(new_period));
                    }
                    AUDCLNT_E_DEVICE_IN_USE | AUDCLNT_E_ENDPOINT_CREATE_FAILED => {
                        debug!("wasapi: transient init error: {:#010x}", e.code().0);
                        return Err(WasapiInitError::Busy);
                    }
                    E_INVALIDARG => {
                        return Err(WasapiInitError::Permanent(anyhow!(
                            "wasapi: initialize: invalid argument"
                        )));
                    }
                    AUDCLNT_E_UNSUPPORTED_FORMAT => {
                        return Err(WasapiInitError::Permanent(anyhow!(
                            "wasapi: initialize: device does not support the audio format"
                        )));
                    }
                    AUDCLNT_E_EXCLUSIVE_MODE_NOT_ALLOWED => {
                        return Err(WasapiInitError::Permanent(anyhow!(
                            "wasapi: initialize: exclusive mode not allowed"
                        )));
                    }
                    code => {
                        return Err(WasapiInitError::Permanent(anyhow!(
                            "wasapi: initialize: HRESULT {:#010x}, {:?}",
                            code.0,
                            e.message()
                        )));
                    }
                }
            }
        }

        self.renderer = Some(
            self.get_renderer()
                .context("wasapi: get_renderer")
                .map_err(WasapiInitError::Permanent)?,
        );
        if !self.pollmode {
            self.eventhandle = Some(
                self.set_get_eventhandle()
                    .context("wasapi: set_event_handle")
                    .map_err(WasapiInitError::Permanent)?,
            );
        }
        Ok(())
    }

    /// Compute the desired period (100ns) for a buffer config.
    pub(crate) fn compute_desired_period(&self, buffer: &BufferConfig) -> Result<i64> {
        let (_default_device_period, min_device_period) = self
            .get_default_and_min_periods()
            .context("wasapi: get_periods")?;
        let target_period_100ns = (buffer.device_period_ms as i64) * 10_000;
        let target_period_100ns = target_period_100ns.max(min_device_period);
        self.calculate_aligned_period_near(target_period_100ns, Some(128))
            .context("wasapi: calculate_aligned_period")
    }

    fn get_renderer(&self) -> Result<AudioRenderClient> {
        Ok(AudioRenderClient(unsafe {
            self.inner_client.GetService::<IAudioRenderClient>()?
        }))
    }

    pub(crate) fn stop(&mut self) -> Result<()> {
        Ok(unsafe {
            self.inner_client.Stop()?;
            self.inner_client.Reset()?
        })
    }

    fn get_available_buffer_frames(&self) -> Result<usize> {
        if self.pollmode {
            Ok(self.max_buffer_frames - unsafe { self.inner_client.GetCurrentPadding()? as usize })
        } else {
            Ok(self.max_buffer_frames)
        }
    }

    pub(crate) fn get_available_buffer_size(&self) -> Result<usize> {
        Ok(self.get_available_buffer_frames()? * self.format.get_block_align() as usize)
    }

    pub(crate) fn wait_for_buffer(&self) -> Result<()> {
        if !self.pollmode {
            if let Some(event) = &self.eventhandle {
                event.wait_for_event(1000)?;
            }
        }
        // In poll mode, the caller's loop already paces writes via
        // get_writable_size() + sleep, so no additional wait is needed.
        Ok(())
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
            renderer: None,
            sharemode,
            max_buffer_frames: 0,
            pollmode: params.pollmode,
            eventhandle: None,
        })
    }

    pub(crate) fn start(&self) -> Result<()> {
        unsafe { self.inner_client.Start()? };
        Ok(())
    }

    fn set_get_eventhandle(&self) -> Result<EventHandle> {
        let handle = unsafe { CreateEventA(None, false, false, PCSTR::null())? };
        unsafe { self.inner_client.SetEventHandle(handle)? };
        Ok(EventHandle(handle))
    }
}

pub struct EventHandle(HANDLE);
impl EventHandle {
    fn wait_for_event(&self, timeout: u32) -> Result<()> {
        let retval = unsafe { WaitForSingleObject(self.0, timeout) };
        if retval.0 != WAIT_OBJECT_0.0 {
            return Err(anyhow!("Wait timed out"));
        }
        Ok(())
    }
}

pub struct AudioRenderClient(IAudioRenderClient);
impl AudioRenderClient {
    fn write(
        &self,
        frames: usize,
        n_block_align: usize,
        data: &[u8],
        buffer_flags: Option<u32>,
    ) -> Result<()> {
        let nbr_bytes = frames * n_block_align;
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
            let buffer_ptr = self.0.GetBuffer(frames as u32)?;
            std::ptr::copy_nonoverlapping(data.as_ptr(), buffer_ptr, nbr_bytes);
            self.0.ReleaseBuffer(frames as u32, flags)?;
        }
        Ok(())
    }
}

/// Struct wrapping a [WAVEFORMATEXTENSIBLE](https://docs.microsoft.com/en-us/windows/win32/api/mmreg/ns-mmreg-waveformatextensible) format descriptor.
#[derive(Clone)]
pub struct WaveFormat(WAVEFORMATEXTENSIBLE);

impl WaveFormat {
    pub(crate) fn new(bits_per_sample: BitsPerSample, samplerate: SampleRate, channels: usize) -> Self {
        let blockalign = channels * bits_per_sample.0 as usize / 8;
        let byterate = samplerate.0 as usize * blockalign;

        let wave_format = WAVEFORMATEX {
            cbSize: 22,
            nAvgBytesPerSec: byterate as u32,
            nBlockAlign: blockalign as u16,
            nChannels: channels as u16,
            nSamplesPerSec: samplerate.0,
            wBitsPerSample: bits_per_sample.0,
            wFormatTag: WAVE_FORMAT_EXTENSIBLE as u16,
        };
        let sample = WAVEFORMATEXTENSIBLE_0 {
            wValidBitsPerSample: bits_per_sample.0,
        };
        let subformat = if bits_per_sample.0 == 32 {
            KSDATAFORMAT_SUBTYPE_IEEE_FLOAT
        } else {
            KSDATAFORMAT_SUBTYPE_PCM
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
        let bits_per_sample = BitsPerSample(wavefmt.wBitsPerSample);
        let samplerate = SampleRate(wavefmt.nSamplesPerSec);
        let channels = wavefmt.nChannels as usize;
        Ok(WaveFormat::new(bits_per_sample, samplerate, channels))
    }

    fn get_samples_per_sec(&self) -> u32 {
        self.0.Format.nSamplesPerSec
    }

    fn get_block_align(&self) -> u16 {
        self.0.Format.nBlockAlign
    }

    fn get_format(&self) -> &WAVEFORMATEX {
        &self.0.Format
    }
}

// WAVEFORMATEX documentation: https://learn.microsoft.com/en-us/windows/win32/api/mmreg/ns-mmreg-waveformatex
// WAVEFORMATEXTENSIBLE documentation: https://docs.microsoft.com/en-us/windows/win32/api/mmreg/ns-mmreg-waveformatextensible
impl From<&StreamParams> for WaveFormat {
    fn from(value: &StreamParams) -> Self {
        WaveFormat::new(
            value.bits_per_sample,
            value.samplerate,
            value.channels as usize,
        )
    }
}

pub struct ThreadPriority {
    previous_process_priority: Option<PROCESS_CREATION_FLAGS>,
    previous_thread_priority: Option<THREAD_PRIORITY>,
    taskhandle: HANDLE,
}

unsafe impl Send for ThreadPriority {}
unsafe impl Sync for ThreadPriority {}

impl ThreadPriority {
    pub fn new(high_priority_mode: bool) -> Result<ThreadPriority> {
        let mut previous_process_priority = None;
        let mut previous_thread_priority = None;
        if high_priority_mode {
            previous_process_priority =
                Some(unsafe { PROCESS_CREATION_FLAGS(GetPriorityClass(GetCurrentProcess())) });
            previous_thread_priority =
                Some(unsafe { THREAD_PRIORITY(GetThreadPriority(GetCurrentThread())) });
            unsafe { SetThreadPriority(GetCurrentThread(), THREAD_PRIORITY_HIGHEST)? };
            unsafe { SetPriorityClass(GetCurrentProcess(), HIGH_PRIORITY_CLASS)? }
        }
        let taskhandle = unsafe { AvSetMmThreadCharacteristicsW(w!("Pro Audio"), &mut 0)? };
        Ok(ThreadPriority {
            previous_process_priority,
            previous_thread_priority,
            taskhandle,
        })
    }

    fn revert_thread_priority(&mut self) -> Result<()> {
        unsafe {
            if let Some(previous_process_priority) = self.previous_process_priority {
                SetPriorityClass(GetCurrentProcess(), previous_process_priority)?;
            }
            if let Some(previous_thread_priority) = self.previous_thread_priority {
                SetThreadPriority(GetCurrentThread(), previous_thread_priority)?;
            }
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
