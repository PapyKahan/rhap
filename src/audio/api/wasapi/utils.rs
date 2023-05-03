use windows::core::HRESULT;
use windows::Win32::Foundation::*;
use windows::Win32::Media::Audio::*;
use windows::Win32::Media::KernelStreaming::WAVE_FORMAT_EXTENSIBLE;

#[inline(always)]
pub(super) fn host_error<'life>(errorcode: HRESULT) -> &'life str {
    match errorcode {
        S_OK => "S_OK",
        E_POINTER => "E_POINTER",
        E_INVALIDARG => "E_INVALIDARG",
        AUDCLNT_E_NOT_INITIALIZED => "AUDCLNT_E_NOT_INITIALIZED",
        AUDCLNT_E_ALREADY_INITIALIZED => "AUDCLNT_E_ALREADY_INITIALIZED",
        AUDCLNT_E_WRONG_ENDPOINT_TYPE => "AUDCLNT_E_WRONG_ENDPOINT_TYPE",
        AUDCLNT_E_DEVICE_INVALIDATED => "AUDCLNT_E_DEVICE_INVALIDATED",
        AUDCLNT_E_NOT_STOPPED => "AUDCLNT_E_NOT_STOPPED",
        AUDCLNT_E_BUFFER_TOO_LARGE => "AUDCLNT_E_BUFFER_TOO_LARGE",
        AUDCLNT_E_OUT_OF_ORDER => "AUDCLNT_E_OUT_OF_ORDER",
        AUDCLNT_E_UNSUPPORTED_FORMAT => "AUDCLNT_E_UNSUPPORTED_FORMAT",
        AUDCLNT_E_INVALID_SIZE => "AUDCLNT_E_INVALID_SIZE",
        AUDCLNT_E_DEVICE_IN_USE => "AUDCLNT_E_DEVICE_IN_USE",
        AUDCLNT_E_BUFFER_OPERATION_PENDING => "AUDCLNT_E_BUFFER_OPERATION_PENDING",
        AUDCLNT_E_THREAD_NOT_REGISTERED => "AUDCLNT_E_THREAD_NOT_REGISTERED",
        AUDCLNT_E_EXCLUSIVE_MODE_NOT_ALLOWED => "AUDCLNT_E_EXCLUSIVE_MODE_NOT_ALLOWED",
        AUDCLNT_E_ENDPOINT_CREATE_FAILED => "AUDCLNT_E_ENDPOINT_CREATE_FAILED",
        AUDCLNT_E_SERVICE_NOT_RUNNING => "AUDCLNT_E_SERVICE_NOT_RUNNING",
        AUDCLNT_E_EVENTHANDLE_NOT_EXPECTED => "AUDCLNT_E_EVENTHANDLE_NOT_EXPECTED",
        AUDCLNT_E_EXCLUSIVE_MODE_ONLY => "AUDCLNT_E_EXCLUSIVE_MODE_ONLY",
        AUDCLNT_E_BUFDURATION_PERIOD_NOT_EQUAL => "AUDCLNT_E_BUFDURATION_PERIOD_NOT_EQUAL",
        AUDCLNT_E_EVENTHANDLE_NOT_SET => "AUDCLNT_E_EVENTHANDLE_NOT_SET",
        AUDCLNT_E_INCORRECT_BUFFER_SIZE => "AUDCLNT_E_INCORRECT_BUFFER_SIZE",
        AUDCLNT_E_BUFFER_SIZE_ERROR => "AUDCLNT_E_BUFFER_SIZE_ERROR",
        AUDCLNT_E_CPUUSAGE_EXCEEDED => "AUDCLNT_E_CPUUSAGE_EXCEEDED",
        AUDCLNT_E_BUFFER_ERROR => "AUDCLNT_E_BUFFER_ERROR",
        AUDCLNT_E_BUFFER_SIZE_NOT_ALIGNED => "AUDCLNT_E_BUFFER_SIZE_NOT_ALIGNED",
        AUDCLNT_E_INVALID_DEVICE_PERIOD => "AUDCLNT_E_INVALID_DEVICE_PERIOD",
        AUDCLNT_E_INVALID_STREAM_FLAG => "AUDCLNT_E_INVALID_STREAM_FLAG",
        AUDCLNT_E_ENDPOINT_OFFLOAD_NOT_CAPABLE => "AUDCLNT_E_ENDPOINT_OFFLOAD_NOT_CAPABLE",
        AUDCLNT_E_OUT_OF_OFFLOAD_RESOURCES => "AUDCLNT_E_OUT_OF_OFFLOAD_RESOURCES",
        AUDCLNT_E_OFFLOAD_MODE_ONLY => "AUDCLNT_E_OFFLOAD_MODE_ONLY",
        AUDCLNT_E_NONOFFLOAD_MODE_ONLY => "AUDCLNT_E_NONOFFLOAD_MODE_ONLY",
        AUDCLNT_E_RESOURCES_INVALIDATED => "AUDCLNT_E_RESOURCES_INVALIDATED",
        AUDCLNT_E_RAW_MODE_UNSUPPORTED => "AUDCLNT_E_RAW_MODE_UNSUPPORTED",
        AUDCLNT_E_ENGINE_PERIODICITY_LOCKED => "AUDCLNT_E_ENGINE_PERIODICITY_LOCKED",
        AUDCLNT_E_ENGINE_FORMAT_LOCKED => "AUDCLNT_E_ENGINE_FORMAT_LOCKED",
        AUDCLNT_S_BUFFER_EMPTY => "AUDCLNT_S_BUFFER_EMPTY",
        AUDCLNT_S_THREAD_ALREADY_REGISTERED => "AUDCLNT_S_THREAD_ALREADY_REGISTERED",
        AUDCLNT_S_POSITION_STALLED => "AUDCLNT_S_POSITION_STALLED",
        _ => "Unknown error",
    }
}

/// Debug fonction to print WAVEFORMATEX values.
#[inline(always)]
#[allow(dead_code)]
pub(super) fn print_wave_format(wave_format: *const WAVEFORMATEX) {
    unsafe {
        let formattag = (*wave_format).wFormatTag;
        println!("Format tag: {:?}", formattag);
        let channels = (*wave_format).nChannels;
        println!("Channels: {:?}", channels);
        let sample_rate = (*wave_format).nSamplesPerSec;
        println!("Sample rate: {:?}", sample_rate);
        let bits_per_sample = (*wave_format).wBitsPerSample;
        println!("Bits per sample: {:?}", bits_per_sample);
        let block_align = (*wave_format).nBlockAlign;
        println!("Block align: {:?}", block_align);
        let bytes_per_second = (*wave_format).nAvgBytesPerSec;
        println!("Bytes per second: {:?}", bytes_per_second);
        let cb_size = (*wave_format).cbSize;
        println!("cbSize: {:?}", cb_size);
        if formattag as u32 == WAVE_FORMAT_EXTENSIBLE {
            let wave_format_extensible = wave_format as *const WAVEFORMATEXTENSIBLE;
            let sub_format = (*wave_format_extensible).SubFormat;
            println!("Sub format: {:?}", sub_format);
            let valid_bits_per_sample = (*wave_format_extensible).Samples.wValidBitsPerSample;
            println!("Valid bits per sample: {:?}", valid_bits_per_sample);
            let channel_mask = (*wave_format_extensible).dwChannelMask;
            println!("Channel mask: {:?}", channel_mask);
        }
    }
}

/// Compute HNS perdiod from frames and samplerate.
#[inline(always)]
pub(super) fn make_hns_period(frames: u32, samplerate: u32) -> u32 {
    10000 * 1000 / samplerate * frames// + 0.5
}

/// Converts HNS period to frames.
#[inline(always)]
pub(super) fn make_frames_from_hns(hns: u32, samplerate: u32) -> u32 {
    hns * samplerate / 1000 / 10000
}

/// Aligns x backward
#[inline(always)]
pub(super) fn align_bwd(x: u32, align: u32) -> u32 {
    x - x % align
}

/// Aligns x forward
#[inline(always)]
pub(super) fn align_fwd(x: u32, align: u32) -> u32 {
    let remainer = x % align;
    if remainer == 0 {
        x
    } else {
        x + (align - remainer)
    }
}

/// Aligns frames per buffer
#[inline(always)]
pub(super) fn align_frames_per_buffer(frames: u32, block_align : u32, alignfn: fn(bytes: u32, align: u32) -> u32) -> u32 {
    let bytes = alignfn(frames * block_align, 128);
    //let bytes = match backward {
    //    true => align_bwd(bytes, align),
    //    false => align_fwd(bytes, align),
    //};
    let bytes = if bytes < 128 { 128 } else { bytes };
    let packets = bytes / 128;
    let bytes = packets * 128;
    let frames = bytes / block_align;
    align_fwd(frames, 8)
}
