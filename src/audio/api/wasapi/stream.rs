use anyhow::Result;
use log::debug;
use log::error;
use windows::Win32::System::Threading::GetCurrentProcess;
use windows::Win32::System::Threading::GetPriorityClass;
use windows::Win32::System::Threading::HIGH_PRIORITY_CLASS;
use windows::Win32::System::Threading::PROCESS_CREATION_FLAGS;
use windows::Win32::System::Threading::SetPriorityClass;
use std::sync::Condvar;
use std::sync::Mutex;
use std::time::Duration;
use tokio::sync::mpsc::Receiver;
use windows::core::w;
use windows::Win32::Foundation::E_INVALIDARG;
use windows::Win32::Foundation::HANDLE;
use windows::Win32::Media::Audio::AUDCLNT_E_BUFFER_SIZE_NOT_ALIGNED;
use windows::Win32::Media::Audio::AUDCLNT_E_DEVICE_IN_USE;
use windows::Win32::Media::Audio::AUDCLNT_E_ENDPOINT_CREATE_FAILED;
use windows::Win32::Media::Audio::AUDCLNT_E_EXCLUSIVE_MODE_NOT_ALLOWED;
use windows::Win32::Media::Audio::AUDCLNT_E_UNSUPPORTED_FORMAT;
use windows::Win32::System::Threading::AvRevertMmThreadCharacteristics;
use windows::Win32::System::Threading::AvSetMmThreadCharacteristicsW;
use windows::Win32::System::Threading::GetCurrentThread;
use windows::Win32::System::Threading::GetThreadPriority;
use windows::Win32::System::Threading::SetThreadPriority;
use windows::Win32::System::Threading::THREAD_PRIORITY;
use windows::Win32::System::Threading::THREAD_PRIORITY_HIGHEST;

use super::api::com_initialize;
use super::api::AudioClient;
use super::api::AudioRenderClient;
use super::api::EventHandle;
use super::api::ShareMode;
use super::api::WaveFormat;
use super::device::Device;
use crate::audio::api::wasapi::api::calculate_period_100ns;
use crate::audio::StreamingData;
use crate::audio::{StreamParams, StreamingCommand};

const REFTIMES_PER_MILLISEC: i64 = 10000;

pub struct Streamer {
    client: AudioClient,
    renderer: AudioRenderClient,
    eventhandle: EventHandle,
    taskhandle: Option<HANDLE>,
    previous_process_priority: Option<PROCESS_CREATION_FLAGS>,
    previous_thread_priority: Option<THREAD_PRIORITY>,
    wave_format: WaveFormat,
    pause_condition: Condvar,
    status: Mutex<StreamingCommand>,
    desired_period: i64,
    command_receiver: Receiver<StreamingCommand>,
    data_receiver: Receiver<StreamingData>,
}

impl Drop for Streamer {
    fn drop(&mut self) {
        self.revert_thread_priority();
    }
}

unsafe impl Send for Streamer {}
unsafe impl Sync for Streamer {}

impl Streamer {

    pub(super) fn new(
        device: &Device,
        data_receiver: Receiver<StreamingData>,
        command_receiver: Receiver<StreamingCommand>,
        params: StreamParams,
    ) -> Result<Self> {
        com_initialize();
        let mut client = device.get_client()?;
        let wave_format = WaveFormat::from(&params);
        let sharemode = match params.exclusive {
            true => ShareMode::Exclusive,
            false => ShareMode::Shared,
        };

        let (_, min_device_period) = client.get_default_and_min_periods()?;

        // Calculate desired period for better device compatibility.
        let mut desired_period = client.calculate_aligned_period_near(
            3 * min_device_period / 2,
            Some(128),
            &wave_format,
        )?;
        let result = client.initialize(&wave_format, desired_period, &sharemode);

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
                            let buffersize = client.get_buffer_size()?;
                            debug!(
                                "Client next-highest-aligned buffer size: {} frames",
                                buffersize
                            );
                            // 2. Call IAudioClient::Release, skipped since this will happen automatically when we drop the client.
                            // 3. Calculate the aligned buffer size in 100-nanosecond units.
                            desired_period = calculate_period_100ns(
                                buffersize as i64,
                                wave_format.get_samples_per_sec() as i64,
                            );
                            debug!("Aligned period in 100ns units: {}", desired_period);
                            // 4. Get a new IAudioClient
                            client = device.get_client()?;
                            // 5. Call Initialize again on the created audio client.
                            client.initialize(&wave_format, desired_period, &sharemode)?;
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
                            error!("IAudioClient::Initialize: Other error, HRESULT: {:#010x}, info: {:?}", werr.code().0, werr.message());
                            panic!("IAudioClient::Initialize: Other error, HRESULT: {:#010x}, info: {:?}", werr.code().0, werr.message());
                        }
                    };
                } else {
                    error!("IAudioClient::Initialize: Other error {:?}", e);
                    panic!("IAudioClient::Initialize failed {:?}", e);
                }
            }
        };

        let eventhandle = client.set_get_eventhandle()?;
        let renderer = client.get_renderer()?;
        Ok(Streamer {
            client,
            renderer,
            eventhandle,
            wave_format,
            desired_period,
            previous_process_priority: None,
            previous_thread_priority: None,
            taskhandle: None,
            pause_condition: Condvar::new(),
            status: Mutex::new(StreamingCommand::None),
            command_receiver,
            data_receiver,
        })
    }

    pub(super) fn wait_readiness(&self) {
        let status = self.status.lock().expect("fail to lock status mutex");
        let _ = self.pause_condition.wait(status);
    }

    fn resume(&self) {
        self.pause_condition.notify_all()
    }

    fn stop(&self) -> Result<()> {
        self.client.stop()
    }

    /// Sets the process and thread priorities for the current audio stream.
    ///
    /// This function performs the following operations:
    /// 1. Stores the current process priority class.
    /// 2. Sets the process priority class to `HIGH_PRIORITY_CLASS`.
    /// 3. Stores the current thread priority.
    /// 4. Sets the thread priority to `THREAD_PRIORITY_HIGHEST`.
    /// 5. Sets the thread characteristics to "Pro Audio".
    ///
    /// # Safety
    ///
    /// This function uses unsafe blocks to call Windows API functions.
    ///
    /// # Errors
    ///
    /// This function will return an error if any of the Windows API function calls fail.
    ///
    /// # Returns
    ///
    /// This function returns `Ok(())` if all operations are successful.
    fn set_process_and_thread_priorities(&mut self) -> Result<()> {
        // Store the current process priority class
        self.previous_process_priority = Some(unsafe { PROCESS_CREATION_FLAGS(GetPriorityClass(GetCurrentProcess())) });
        // Set the process priority class to HIGH_PRIORITY_CLASS
        unsafe { SetPriorityClass(GetCurrentProcess(), HIGH_PRIORITY_CLASS)?}
        // Store the current thread priority
        self.previous_thread_priority = Some(unsafe { THREAD_PRIORITY(GetThreadPriority(GetCurrentThread())) });
        // Set the thread priority to THREAD_PRIORITY_HIGHEST
        unsafe { SetThreadPriority(GetCurrentThread(), THREAD_PRIORITY_HIGHEST)? };
        // Set the thread characteristics to "Pro Audio"
        self.taskhandle = Some(unsafe { AvSetMmThreadCharacteristicsW(w!("Pro Audio"), &mut 0)? });
        Ok(())
    }

    fn revert_thread_priority(&mut self) {
        if let Some(previous_priority) = self.previous_process_priority.take() {
            let _ = unsafe { SetPriorityClass(GetCurrentProcess(), previous_priority) };
        }
        if let Some(previous_priority) = self.previous_thread_priority.take() {
            let _ = unsafe { SetThreadPriority(GetCurrentThread(), previous_priority) };
        }
        if let Some(handle) = self.taskhandle.take() {
            let _ = unsafe { AvRevertMmThreadCharacteristics(handle) };
        }
    }

    pub(crate) async fn start(&mut self) -> Result<()> {
        com_initialize();
        self.set_process_and_thread_priorities()?;

        let mut buffer = vec![];
        let mut stream_started = false;
        let (mut available_buffer_in_frames, mut available_buffer_size) = self.client.get_available_buffer_size()?;

        loop {
            match self.command_receiver.try_recv() {
                Ok(command) => match command {
                    StreamingCommand::Pause => self.pause()?,
                    StreamingCommand::Resume => self.resume(),
                    _ => {}
                },
                Err(tokio::sync::mpsc::error::TryRecvError::Empty) => {}
                Err(tokio::sync::mpsc::error::TryRecvError::Disconnected) => break,
            }

            if let Some(streaming_data) = self.data_receiver.recv().await {
                let data = match streaming_data {
                    StreamingData::Data(data) => data,
                    StreamingData::EndOfStream => break,
                };
                buffer.push(data);
                if buffer.len() != available_buffer_size {
                    continue;
                }

                self.renderer.write(
                    available_buffer_in_frames,
                    self.wave_format.get_block_align() as usize,
                    buffer.as_slice(),
                    None,
                )?;

                if !stream_started {
                    self.client.start()?;
                    stream_started = !stream_started;
                }

                self.eventhandle.wait_for_event(1000)?;
                buffer.clear();
                (available_buffer_in_frames, available_buffer_size) = self.client.get_available_buffer_size()?;
            } else {
                let bytes_per_frames = self.wave_format.get_block_align() as usize;
                let frames = buffer.len() / bytes_per_frames;
                self.renderer
                    .write(frames as usize, bytes_per_frames, buffer.as_slice(), None)?;
                tokio::time::sleep(Duration::from_millis(
                    self.desired_period as u64 / REFTIMES_PER_MILLISEC as u64,
                ))
                .await;
                break;
            }
        }
        self.revert_thread_priority();
        self.stop()
    }

    fn pause(&self) -> Result<()> {
        self.client.stop()?;
        self.wait_readiness();
        self.client.start()
    }
}
