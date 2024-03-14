use std::{cmp, slice};
use num_integer::Integer;
use windows::{core::PCSTR, Win32::{Foundation::{HANDLE, RPC_E_CHANGED_MODE, WAIT_OBJECT_0}, Media::{Audio::{IAudioClient, IAudioRenderClient, AUDCLNT_SHAREMODE_EXCLUSIVE, AUDCLNT_SHAREMODE_SHARED, AUDCLNT_STREAMFLAGS_EVENTCALLBACK, WAVEFORMATEX, WAVEFORMATEXTENSIBLE, WAVEFORMATEXTENSIBLE_0}, KernelStreaming::{KSDATAFORMAT_SUBTYPE_PCM, WAVE_FORMAT_EXTENSIBLE}, Multimedia::KSDATAFORMAT_SUBTYPE_IEEE_FLOAT}, System::{Com::{CoInitializeEx, CoUninitialize, COINIT_MULTITHREADED}, Threading::{CreateEventA, WaitForSingleObject}}}};
use anyhow::{anyhow, Result};

use crate::audio::BitsPerSample;

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
    is_ok: bool
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

pub struct AudioClient
{
    pub inner_client: IAudioClient,
    sharemode: Option<ShareMode>,
}

impl AudioClient {
    pub fn is_supported(&self, format: WaveFormat, share_mode : &ShareMode) -> Result<WaveFormat> {
        match share_mode {
            ShareMode::Exclusive => self.is_supported_exclusive(format),
            ShareMode::Shared => self.is_supported_shared(format),
        }
    }

    fn is_supported_exclusive(&self, format: WaveFormat) -> Result<WaveFormat> {
        let first_test = unsafe {
            self.inner_client.IsFormatSupported(
                    AUDCLNT_SHAREMODE_EXCLUSIVE,
                    &format.0.Format,
                    None,
                ).ok()
        };
        if first_test.is_ok() {
            return Ok(format);
        }
        //perform a second test with WAVEFORMATEX if channel mask is less than 2
        if format.0.dwChannelMask <= 2 {
            let wave_format = format.0.Format.clone();
            unsafe {
                self.inner_client.IsFormatSupported(
                    AUDCLNT_SHAREMODE_EXCLUSIVE,
                    &wave_format,
                    None,
                ).ok()?
            };
            return Ok(format);
        }
        Err(anyhow!("Format not supported"))
    }

    fn is_supported_shared(&self, format: WaveFormat) -> Result<WaveFormat> {
        let mut closest_match: *mut WAVEFORMATEX = std::ptr::null_mut();
        let result = unsafe {
            self.inner_client.IsFormatSupported(
                AUDCLNT_SHAREMODE_SHARED,
                &format.0.Format,
                Some(&mut closest_match),
            ).ok()
        };
        if result.is_ok() {
            return Ok(format);
        } else {
            let fmt: WAVEFORMATEX = unsafe { closest_match.read() };
            Ok(WaveFormat::from_waveformatex(fmt)?)
        }
        
    }

    pub fn get_min_and_default_periods(&self) -> Result<(i64, i64)> {
        let mut default_period = 0;
        let mut min_period = 0;
        unsafe {
            self.inner_client.GetDevicePeriod(Some(&mut default_period), Some(&mut min_period))?
        };
        Ok((default_period, min_period))
    }

    pub fn calculate_aligned_period_near(
        &self,
        desired_period: i64,
        align_bytes: Option<u32>,
        wave_fmt: &WaveFormat,
    ) -> Result<i64> {
        let (_, min_period) = self.get_min_and_default_periods()?;
        let adjusted_period = cmp::max(desired_period, min_period);
        let frame_bytes = wave_fmt.0.Format.nBlockAlign as u32;
        let period_alignment_bytes = match align_bytes {
            Some(0) => frame_bytes,
            Some(bytes) => frame_bytes.lcm(&bytes),
            None => frame_bytes,
        };
        let period_alignment_frames = period_alignment_bytes as i64 / frame_bytes as i64;
        let desired_period_frames =
            (adjusted_period as f64 * wave_fmt.0.Format.nSamplesPerSec as f64 / 10000000.0)
                .round() as i64;
        let min_period_frames =
            (min_period as f64 * wave_fmt.0.Format.nSamplesPerSec as f64 / 10000000.0).ceil() as i64;
        let mut nbr_segments = desired_period_frames / period_alignment_frames;
        if nbr_segments * period_alignment_frames < min_period_frames {
            // Add one segment if the value got rounded down below the minimum
            nbr_segments += 1;
        }
        let aligned_period = calculate_period_100ns(
            period_alignment_frames * nbr_segments,
            wave_fmt.0.Format.nSamplesPerSec as i64,
        );
        Ok(aligned_period)
    }
    
    pub(crate) fn initialize(&mut self, format: &WaveFormat, desired_period: i64, sharemode: &ShareMode) -> Result<()> {
        self.sharemode = Some(sharemode.clone());
        let mode = match sharemode {
            ShareMode::Exclusive => AUDCLNT_SHAREMODE_EXCLUSIVE,
            ShareMode::Shared => AUDCLNT_SHAREMODE_SHARED,
        };
        let device_period = match sharemode {
            ShareMode::Exclusive => desired_period,
            ShareMode::Shared => 0,
        };
        let flags = match sharemode {
            ShareMode::Exclusive => AUDCLNT_STREAMFLAGS_EVENTCALLBACK,
            ShareMode::Shared => AUDCLNT_STREAMFLAGS_EVENTCALLBACK,
        };
        unsafe {
            self.inner_client.Initialize(
                mode,
                flags,
                desired_period,
                device_period,
                &format.0.Format,
                None,
            )?;
        }
        Ok(())
    }
    
    pub(crate) fn get_buffer_size(&self) -> Result<usize> {
        Ok(unsafe { self.inner_client.GetBufferSize()? as usize})
    }
    
    pub(crate) fn get_renderer(&self) -> Result<AudioRenderClient> {
        Ok(AudioRenderClient(unsafe { self.inner_client.GetService::<IAudioRenderClient>()? }))
    }
    
    pub(crate) fn stop(&self) -> Result<()> {
        Ok(unsafe { self.inner_client.Stop()? })
    }
    
    pub(crate) fn get_available_frames(&self) -> Result<usize> {
        let frames = match self.sharemode {
            Some(ShareMode::Exclusive) => {
                let buffer_frame_count = unsafe { self.inner_client.GetBufferSize()? as usize };
                buffer_frame_count
            },
            Some(ShareMode::Shared) => {
                let padding_count = unsafe { self.inner_client.GetCurrentPadding()? as usize };
                let buffer_frame_count = unsafe { self.inner_client.GetBufferSize()? as usize };
                buffer_frame_count - padding_count
            }
            _ => return Err(anyhow!("Client has not been initialized")),
        };
        Ok(frames)
    }
    
    pub(crate) fn new(none: IAudioClient) -> Result<AudioClient> {
        Ok(AudioClient {
            inner_client: none,
            sharemode: None,
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
    pub(crate) fn write(&self, available_frames: usize, n_block_align: usize, data: &[u8], buffer_flags: Option<u32>) -> Result<()> {
        let nbr_bytes = available_frames * n_block_align;
        if nbr_bytes != data.len() {
            return Err(anyhow!(
                    "Wrong length of data, got {}, expected {}",
                    data.len(),
                    nbr_bytes
            ));
        }
        let bufferptr = unsafe { self.0.GetBuffer(available_frames as u32)? };
        let bufferslice = unsafe { slice::from_raw_parts_mut(bufferptr, nbr_bytes) };
        bufferslice.copy_from_slice(data);
        let flags = match buffer_flags {
            Some(bflags) => bflags,
            None => 0,
        };
        unsafe { self.0.ReleaseBuffer(available_frames as u32, flags)? };
        Ok(())
    }
}

/// Struct wrapping a [WAVEFORMATEXTENSIBLE](https://docs.microsoft.com/en-us/windows/win32/api/mmreg/ns-mmreg-waveformatextensible) format descriptor.
#[derive(Clone)]
pub struct WaveFormat(WAVEFORMATEXTENSIBLE);

impl WaveFormat {
    pub fn new(
        bits_per_sample: BitsPerSample,
        samplerate: usize,
        channels: usize,
        channel_mask: Option<u32>,
    ) -> Self {
        let blockalign = channels * bits_per_sample as usize / 8;
        let byterate = samplerate * blockalign;

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
            wValidBitsPerSample: bits_per_sample as u16,
        };
        let subformat = match bits_per_sample {
            BitsPerSample::Bits8 => KSDATAFORMAT_SUBTYPE_PCM,
            BitsPerSample::Bits16 => KSDATAFORMAT_SUBTYPE_PCM,
            BitsPerSample::Bits24 => KSDATAFORMAT_SUBTYPE_PCM,
            BitsPerSample::Bits32 => KSDATAFORMAT_SUBTYPE_IEEE_FLOAT,
        };
        // https://docs.microsoft.com/en-us/windows/win32/api/mmreg/ns-mmreg-waveformatextensible
        let mask = if let Some(given_mask) = channel_mask {
            given_mask
        } else {
            match channels {
                ch if ch <= 18 => {
                    // setting bit for each channel
                    (1 << ch) - 1
                }
                _ => 0,
            }
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
    pub fn from_waveformatex(wavefmt: WAVEFORMATEX) -> Result<Self> {
        let bits_per_sample = BitsPerSample::from(wavefmt.wBitsPerSample as usize);
        let samplerate = wavefmt.nSamplesPerSec as usize;
        let channels = wavefmt.nChannels as usize;
        Ok(WaveFormat::new(
            bits_per_sample,
            samplerate,
            channels,
            None,
        ))
    }

    pub(crate) fn get_samples_per_sec(&self) -> u32 {
        self.0.Format.nSamplesPerSec
    }

    pub(crate) fn get_block_align(&self) -> u16 {
        self.0.Format.nBlockAlign
    }
}
