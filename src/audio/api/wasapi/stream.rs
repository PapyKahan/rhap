use anyhow::Result;
use std::time::Duration;
use tokio::sync::mpsc::Receiver;
use windows::core::w;
use windows::Win32::Foundation::HANDLE;
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

use super::api::com_initialize;
use super::api::AudioClient;
use super::api::AudioRenderClient;
use super::api::EventHandle;
use super::api::ShareMode;
use super::api::WaveFormat;
use super::device::Device;
use crate::audio::StreamParams;
use crate::audio::StreamingData;

const REFTIMES_PER_MILLISEC: i64 = 10000;

pub struct Streamer {
    client: AudioClient,
    renderer: AudioRenderClient,
    eventhandle: EventHandle,
    taskhandle: Option<HANDLE>,
    previous_process_priority: Option<PROCESS_CREATION_FLAGS>,
    previous_thread_priority: Option<THREAD_PRIORITY>,
    wave_format: WaveFormat,
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
        params: StreamParams,
    ) -> Result<Self> {
        com_initialize();
        let mut client = device.get_client()?;
        let format = WaveFormat::from(&params);
        let sharemode = match params.exclusive {
            true => ShareMode::Exclusive,
            false => ShareMode::Shared,
        };

        client.initialize(&format, &sharemode)?;
        let eventhandle = client.set_get_eventhandle()?;
        let renderer = client.get_renderer()?;
        Ok(Streamer {
            client,
            renderer,
            eventhandle,
            wave_format: format,
            previous_process_priority: None,
            previous_thread_priority: None,
            taskhandle: None,
            data_receiver,
        })
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
        self.previous_process_priority =
            Some(unsafe { PROCESS_CREATION_FLAGS(GetPriorityClass(GetCurrentProcess())) });
        // Set the process priority class to HIGH_PRIORITY_CLASS
        unsafe { SetPriorityClass(GetCurrentProcess(), HIGH_PRIORITY_CLASS)? }
        // Store the current thread priority
        self.previous_thread_priority =
            Some(unsafe { THREAD_PRIORITY(GetThreadPriority(GetCurrentThread())) });
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
        let (mut available_buffer_in_frames, mut available_buffer_size) =
            self.client.get_available_buffer_size()?;

        loop {
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
                (available_buffer_in_frames, available_buffer_size) =
                    self.client.get_available_buffer_size()?;
            } else {
                let bytes_per_frames = self.wave_format.get_block_align() as usize;
                let frames = buffer.len() / bytes_per_frames;
                self.renderer
                    .write(frames as usize, bytes_per_frames, buffer.as_slice(), None)?;
                tokio::time::sleep(Duration::from_millis(
                    self.client.get_period() as u64 / REFTIMES_PER_MILLISEC as u64,
                ))
                .await;
                break;
            }
        }
        self.revert_thread_priority();
        self.stop()
    }
}
