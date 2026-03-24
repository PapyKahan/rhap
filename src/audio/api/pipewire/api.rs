use anyhow::{anyhow, Result};
use log::{debug, error};
use pipewire as pw;
use pw::properties::properties;
use pw::spa;
use pw::stream::{StreamBox, StreamFlags};
use ringbuf::traits::{Consumer, Observer};
use ringbuf::HeapCons;
use spa::param::audio::{AudioFormat, AudioInfoRaw};
use spa::pod::{Object, Pod, Value};
use spa::utils::Direction;
use std::cell::RefCell;
use std::io::Cursor;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use crate::audio::device::BufferSignal;
use crate::audio::{BitsPerSample, StreamParams};

static PIPEWIRE_INIT: std::sync::Once = std::sync::Once::new();

pub fn init_pipewire() {
    PIPEWIRE_INIT.call_once(|| {
        pw::init();
    });
}

pub enum StreamCommand {
    Cork(bool),
    Quit,
}

struct CallbackState {
    consumer: HeapCons<u8>,
    end_of_stream: Arc<AtomicBool>,
    signal: Arc<BufferSignal>,
    bytes_per_frame: usize,
    done: bool,
}

pub struct PwStreamHandle {
    thread: Option<std::thread::JoinHandle<()>>,
    sender: pw::channel::Sender<StreamCommand>,
}

impl PwStreamHandle {
    pub fn cork(&self, corked: bool) {
        let _ = self.sender.send(StreamCommand::Cork(corked));
    }

    pub fn stop(&mut self) {
        let _ = self.sender.send(StreamCommand::Quit);
        if let Some(handle) = self.thread.take() {
            match handle.join() {
                Ok(()) => {}
                Err(_) => error!("PipeWire audio thread panicked"),
            }
        }
    }
}

pub fn start_stream(
    params: &StreamParams,
    consumer: HeapCons<u8>,
    end_of_stream: Arc<AtomicBool>,
    signal: Arc<BufferSignal>,
    node_id: Option<u32>,
) -> Result<PwStreamHandle> {
    init_pipewire();

    let audio_format = bits_to_spa_format(params.bits_per_sample)?;
    let sample_rate = params.samplerate.0;
    let channels = params.channels as u32;
    let bytes_per_frame = (params.bits_per_sample.0 / 8) as usize * channels as usize;

    let (cmd_sender, cmd_receiver) = pw::channel::channel::<StreamCommand>();

    let thread = std::thread::Builder::new()
        .name("rhap-audio-out".into())
        .spawn(move || {
            if let Err(e) = run_pipewire_loop(
                audio_format,
                sample_rate,
                channels,
                bytes_per_frame,
                consumer,
                end_of_stream,
                signal,
                node_id,
                cmd_receiver,
            ) {
                error!("PipeWire audio loop error: {:#}", e);
            }
        })?;

    Ok(PwStreamHandle {
        thread: Some(thread),
        sender: cmd_sender,
    })
}

fn bits_to_spa_format(bits: BitsPerSample) -> Result<AudioFormat> {
    match bits.0 {
        16 => Ok(AudioFormat::S16LE),
        24 => Ok(AudioFormat::S24LE),
        32 => Ok(AudioFormat::F32LE),
        other => Err(anyhow!("Unsupported bits per sample for PipeWire: {}", other)),
    }
}

fn build_audio_format_params(
    audio_format: AudioFormat,
    sample_rate: u32,
    channels: u32,
) -> Vec<u8> {
    let mut info = AudioInfoRaw::new();
    info.set_format(audio_format);
    info.set_rate(sample_rate);
    info.set_channels(channels);

    let value = Value::Object(Object {
        type_: pw::spa::sys::SPA_TYPE_OBJECT_Format,
        id: pw::spa::sys::SPA_PARAM_EnumFormat,
        properties: info.into(),
    });

    pw::spa::pod::serialize::PodSerializer::serialize(Cursor::new(Vec::new()), &value)
        .expect("Failed to serialize audio format POD")
        .0
        .into_inner()
}

fn run_pipewire_loop(
    audio_format: AudioFormat,
    sample_rate: u32,
    channels: u32,
    bytes_per_frame: usize,
    consumer: HeapCons<u8>,
    end_of_stream: Arc<AtomicBool>,
    signal: Arc<BufferSignal>,
    node_id: Option<u32>,
    cmd_receiver: pw::channel::Receiver<StreamCommand>,
) -> Result<()> {
    let main_loop = pw::main_loop::MainLoopBox::new(None)?;
    let context = pw::context::ContextBox::new(main_loop.loop_(), None)?;
    let core = context.connect(None)?;

    let state = Rc::new(RefCell::new(CallbackState {
        consumer,
        end_of_stream,
        signal,
        bytes_per_frame,
        done: false,
    }));

    let props = properties! {
        *pw::keys::MEDIA_TYPE => "Audio",
        *pw::keys::MEDIA_ROLE => "Music",
        *pw::keys::MEDIA_CATEGORY => "Playback",
        *pw::keys::NODE_NAME => "rhap",
        *pw::keys::APP_NAME => "rhap",
        "node.passthrough" => "true",
    };

    let stream = StreamBox::new(&core, "rhap", props)?;

    let state_for_process = Rc::clone(&state);
    // SAFETY: main_loop outlives all closures — both are on this thread and
    // the closures only run during main_loop.run().
    let main_loop_ptr: *const pw::main_loop::MainLoop = &*main_loop;

    let _listener = stream
        .add_local_listener_with_user_data(())
        .process(move |stream: &pw::stream::Stream, _| {
            let mut st = state_for_process.borrow_mut();

            if st.done {
                return;
            }

            let mut buf = match stream.dequeue_buffer() {
                Some(b) => b,
                None => return,
            };

            let datas = buf.datas_mut();
            if datas.is_empty() {
                return;
            }
            let data = &mut datas[0];
            let max_bytes = data.as_raw().maxsize as usize;

            let dest = match data.data() {
                Some(d) => d,
                None => return,
            };

            let available = st.consumer.occupied_len();

            if available >= st.bytes_per_frame {
                let to_read = {
                    let raw = std::cmp::min(available, max_bytes);
                    // Round down to a frame boundary so we never split a sample.
                    (raw / st.bytes_per_frame) * st.bytes_per_frame
                };
                let n = st.consumer.pop_slice(&mut dest[..to_read]);
                if n > 0 {
                    st.signal.notify();
                }
                let chunk = data.chunk_mut();
                *chunk.offset_mut() = 0;
                *chunk.stride_mut() = st.bytes_per_frame as i32;
                *chunk.size_mut() = n as u32;
            } else if st.end_of_stream.load(Ordering::Acquire) {
                let remaining = st.consumer.occupied_len();
                if remaining > 0 {
                    let to_read = std::cmp::min(remaining, max_bytes);
                    let n = st.consumer.pop_slice(&mut dest[..to_read]);
                    if n > 0 {
                        st.signal.notify();
                    }
                    dest[n..max_bytes].fill(0);
                    let chunk = data.chunk_mut();
                    *chunk.offset_mut() = 0;
                    *chunk.stride_mut() = st.bytes_per_frame as i32;
                    *chunk.size_mut() = max_bytes as u32;
                } else {
                    dest[..max_bytes].fill(0);
                    let chunk = data.chunk_mut();
                    *chunk.offset_mut() = 0;
                    *chunk.stride_mut() = st.bytes_per_frame as i32;
                    *chunk.size_mut() = max_bytes as u32;
                    st.done = true;
                    // SAFETY: main_loop_ptr is valid for the duration of run()
                    unsafe { (*main_loop_ptr).quit() };
                }
            } else {
                dest[..max_bytes].fill(0);
                let chunk = data.chunk_mut();
                *chunk.offset_mut() = 0;
                *chunk.stride_mut() = st.bytes_per_frame as i32;
                *chunk.size_mut() = max_bytes as u32;
            }
        })
        .register()?;

    let pod_bytes = build_audio_format_params(audio_format, sample_rate, channels);
    let pod = Pod::from_bytes(&pod_bytes).ok_or_else(|| anyhow!("Invalid audio format POD"))?;

    stream.connect(
        Direction::Output,
        node_id,
        StreamFlags::AUTOCONNECT | StreamFlags::MAP_BUFFERS | StreamFlags::RT_PROCESS,
        &mut [pod],
    )?;

    // SAFETY: stream_ptr is valid for the duration of main_loop.run() below.
    // Raw pointers are required because StreamBox<'c> is not 'static; the 'c
    // lifetime is purely a Rust safety fiction — the underlying pw_stream lives
    // as long as stream does, which outlasts the closures.
    let stream_ptr: *const pw::stream::Stream = &*stream;
    let _cmd_listener = cmd_receiver.attach(main_loop.loop_(), move |cmd| match cmd {
        StreamCommand::Cork(corked) => {
            let _ = unsafe { (*stream_ptr).set_active(!corked) };
        }
        StreamCommand::Quit => {
            let _ = unsafe { (*stream_ptr).disconnect() };
            unsafe { (*main_loop_ptr).quit() };
        }
    });

    debug!("PipeWire stream connected, starting main loop");
    main_loop.run();
    debug!("PipeWire main loop exited");

    Ok(())
}
