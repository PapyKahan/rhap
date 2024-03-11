use anyhow::{anyhow, Result};
use log::debug;
use log::error;
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

use super::api::AudioClient;
use super::api::AudioRenderClient;
use super::api::EventHandle;
use super::api::ShareMode;
use super::api::WaveFormat;
use super::api::com_initialize;
use super::device::Device;
use crate::audio::StreamingData;
use crate::audio::api::wasapi::api::calculate_period_100ns;
use crate::audio::{StreamParams, StreamingCommand};

const REFTIMES_PER_MILLISEC: i64 = 10000;

pub struct Streamer {
    client: AudioClient,
    renderer: AudioRenderClient,
    eventhandle: EventHandle,
    taskhandle: Option<HANDLE>,
    wave_format: WaveFormat,
    pause_condition: Condvar,
    status: Mutex<StreamingCommand>,
    desired_period: i64,
    command_receiver: Receiver<StreamingCommand>,
    data_receiver: Receiver<StreamingData>,
}

impl Drop for Streamer {
    fn drop(&mut self) {
        if let Some(handle) = self.taskhandle.take() {
            unsafe {
                let _ = windows::Win32::System::Threading::AvRevertMmThreadCharacteristics(handle);
            }
        }
    }
}

unsafe impl Send for Streamer {}
unsafe impl Sync for Streamer {}

impl Streamer {
    // WAVEFORMATEX documentation: https://learn.microsoft.com/en-us/windows/win32/api/mmreg/ns-mmreg-waveformatex
    // WAVEFORMATEXTENSIBLE documentation: https://docs.microsoft.com/en-us/windows/win32/api/mmreg/ns-mmreg-waveformatextensible
    #[inline(always)]
    pub(super) fn create_waveformat_from(params: &StreamParams) -> WaveFormat {
        WaveFormat::new(
            params.bits_per_sample,
            params.samplerate as usize,
            params.channels as usize,
            None,
        )
    }

    pub(super) fn new(
        device: &Device,
        data_receiver: Receiver<StreamingData>,
        command_receiver: Receiver<StreamingCommand>,
        params: StreamParams,
    ) -> Result<Self> {
        com_initialize();
        let mut client = device.get_client()?;
        let wave_format = Streamer::create_waveformat_from(&params);
        let sharemode = match params.exclusive {
            true => ShareMode::Exclusive,
            false => ShareMode::Shared,
        };

        let (_, min_device_period) = client
            .get_min_and_default_periods()?;
        let default_device_period = if params.buffer_length != 0 {
            (params.buffer_length * 1000000) / 100 as i64
        } else {
            min_device_period
        };

        // Calculate desired period for better device compatibility.
        let mut desired_period = client
            .calculate_aligned_period_near(3 * default_device_period / 2, Some(128), &wave_format)?;
        let result = client.initialize(
            &wave_format,
            desired_period,
            &sharemode,
        );

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
                                wave_format.0.Format.nSamplesPerSec as i64
                            );
                            debug!("Aligned period in 100ns units: {}", desired_period);
                            // 4. Get a new IAudioClient
                            client = device.get_client()?;
                            // 5. Call Initialize again on the created audio client.
                            client
                                .initialize(
                                    &wave_format,
                                    desired_period,
                                    &sharemode,
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
        self
            .client
            .stop()
    }

    pub(crate) async fn start(&mut self) -> Result<()> {
        com_initialize();
        let mut buffer = vec![];

        self.taskhandle = Some(unsafe {
            windows::Win32::System::Threading::AvSetMmThreadCharacteristicsW(
                w!("Pro Audio"),
                &mut 0,
            )
            .map_err(|e| anyhow!("AvSetMmThreadCharacteristics failed: {:?}", e))?
        });

        let mut stream_started = false;
        let mut available_frames = self
            .client
            .get_available_frames()?;
        let mut available_buffer_len =
            available_frames as usize * self.wave_format.0.Format.nBlockAlign as usize;

        loop {
            match self.command_receiver.try_recv() {
                Ok(command) => match command {
                    StreamingCommand::Pause => self.pause()?,
                    StreamingCommand::Resume => self.resume(),
                    StreamingCommand::Stop => break,
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
                if buffer.len() != available_buffer_len {
                    continue;
                }

                self.renderer
                    .write(
                        available_frames as usize,
                        self.wave_format.0.Format.nBlockAlign as usize,
                        buffer.as_slice(),
                        None,
                    )?;

                if !stream_started {
                    self.client.start()?;
                    stream_started = !stream_started;
                }

                self.eventhandle.wait_for_event(1000)?;
                buffer.clear();
                available_frames = self.client.get_available_frames()?;
                available_buffer_len =
                    available_frames as usize * self.wave_format.0.Format.nBlockAlign as usize;

            } else {
                let bytes_per_frames = self.wave_format.0.Format.nBlockAlign as usize;
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
        if let Some(handle) = self.taskhandle.take() {
            unsafe {
                windows::Win32::System::Threading::AvRevertMmThreadCharacteristics(handle)
                    .map_err(|e| anyhow!("AvRevertMmThreadCharacteristics failed: {:?}", e))?;
            }
        }
        self.stop()
    }

    fn pause(&self) -> Result<()> {
        self.client.stop()?;
        self.wait_readiness();
        self.client.start()
    }
}
