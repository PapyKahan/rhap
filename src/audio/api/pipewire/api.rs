use anyhow::{anyhow, Context, Result};
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
    /// True when we forced the daemon's clock.force-rate at startup; we must
    /// reset it to 0 on stop so PipeWire is left in its normal auto-rate mode
    /// for other apps. Only set when exclusive (bit-perfect) playback was
    /// requested.
    force_rate_active: bool,
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
        if self.force_rate_active {
            reset_clock_force_rate();
            self.force_rate_active = false;
        }
    }
}

/// Force the PipeWire daemon's graph clock to a specific rate, so the entire
/// graph (and the target sink behind it) runs at the track's native rate.
/// This is the same mechanism QBZ uses for bit-perfect playback. Returns
/// true if the metadata was set successfully.
fn set_clock_force_rate(rate: u32) -> bool {
    match std::process::Command::new("pw-metadata")
        .args(["-n", "settings", "0", "clock.force-rate", &rate.to_string()])
        .output()
    {
        Ok(o) if o.status.success() => true,
        Ok(o) => {
            error!(
                "pw-metadata clock.force-rate={} failed: {}",
                rate,
                String::from_utf8_lossy(&o.stderr)
            );
            false
        }
        Err(e) => {
            error!("pw-metadata not runnable (is it installed?): {}", e);
            false
        }
    }
}

/// Restore PipeWire's graph clock to auto-negotiation. Called on stream stop
/// to leave the daemon in a normal state for other applications.
fn reset_clock_force_rate() {
    let _ = std::process::Command::new("pw-metadata")
        .args(["-n", "settings", "0", "clock.force-rate", "0"])
        .output();
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
    let exclusive = params.exclusive;

    // Bit-perfect path: force PipeWire's graph clock to the track's rate
    // BEFORE creating the stream, then give the daemon a moment to switch
    // before we connect. This is the mechanism QBZ uses; it's the only
    // approach that has worked reliably across PipeWire/WirePlumber versions.
    let force_rate_active = if exclusive {
        let ok = set_clock_force_rate(sample_rate);
        if ok {
            std::thread::sleep(std::time::Duration::from_millis(500));
        }
        ok
    } else {
        false
    };

    let (cmd_sender, cmd_receiver) = pw::channel::channel::<StreamCommand>();

    let thread = std::thread::Builder::new()
        .name("rhap-audio-out".into())
        .spawn(move || {
            if let Err(e) = run_pipewire_loop(
                audio_format,
                sample_rate,
                channels,
                bytes_per_frame,
                exclusive,
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
        force_rate_active,
    })
}

fn bits_to_spa_format(bits: BitsPerSample) -> Result<AudioFormat> {
    match bits.0 {
        16 => Ok(AudioFormat::S16LE),
        24 => Ok(AudioFormat::S24LE),
        // 32-bit uses IEEE float — matches WASAPI (KSDATAFORMAT_SUBTYPE_IEEE_FLOAT)
        // and Symphonia's RawSampleBuffer<f32> output in the ring buffer.
        32 => Ok(AudioFormat::F32LE),
        other => Err(anyhow!("Unsupported bits per sample for PipeWire: {}", other)),
    }
}

fn build_audio_format_params(
    audio_format: AudioFormat,
    sample_rate: u32,
    channels: u32,
) -> Result<Vec<u8>> {
    let mut info = AudioInfoRaw::new();
    info.set_format(audio_format);
    info.set_rate(sample_rate);
    info.set_channels(channels);

    let value = Value::Object(Object {
        type_: pw::spa::sys::SPA_TYPE_OBJECT_Format,
        id: pw::spa::sys::SPA_PARAM_EnumFormat,
        properties: info.into(),
    });

    let (cursor, _) = pw::spa::pod::serialize::PodSerializer::serialize(
        Cursor::new(Vec::new()),
        &value,
    )
    .map_err(|e| anyhow!("Failed to serialize audio format POD: {:?}", e))?;
    Ok(cursor.into_inner())
}

fn run_pipewire_loop(
    audio_format: AudioFormat,
    sample_rate: u32,
    channels: u32,
    bytes_per_frame: usize,
    exclusive: bool,
    consumer: HeapCons<u8>,
    end_of_stream: Arc<AtomicBool>,
    signal: Arc<BufferSignal>,
    node_id: Option<u32>,
    cmd_receiver: pw::channel::Receiver<StreamCommand>,
) -> Result<()> {
    let main_loop = pw::main_loop::MainLoopBox::new(None).context("pw: main_loop")?;
    let context = pw::context::ContextBox::new(main_loop.loop_(), None).context("pw: context")?;
    let core = context.connect(None).context("pw: connect_core")?;

    let state = Rc::new(RefCell::new(CallbackState {
        consumer,
        end_of_stream,
        signal,
        bytes_per_frame,
        done: false,
    }));

    // Bit-perfect playback is achieved by forcing the graph clock at the
    // daemon level (see set_clock_force_rate in start_stream). Stream-level
    // props like node.passthrough / node.exclusive / node.force-rate look
    // appealing but reliably trigger "no target node available" because
    // WirePlumber's policy refuses or because passthrough requires sink
    // support for compressed audio. We only set node.rate as a hint here,
    // which is harmless if the daemon already forced the rate.
    let mut props = properties! {
        *pw::keys::MEDIA_TYPE => "Audio",
        *pw::keys::MEDIA_ROLE => "Music",
        *pw::keys::MEDIA_CATEGORY => "Playback",
        *pw::keys::NODE_NAME => "rhap",
        *pw::keys::APP_NAME => "rhap",
    };
    props.insert("node.rate", format!("1/{}", sample_rate));
    let _ = exclusive;

    let stream = StreamBox::new(&core, "rhap", props).context("pw: stream_new")?;

    let state_for_process = Rc::clone(&state);
    // SAFETY: main_loop outlives all closures — both are on this thread and
    // the closures only run during main_loop.run().
    let main_loop_ptr: *const pw::main_loop::MainLoop = &*main_loop;

    let _listener = stream
        .add_local_listener_with_user_data(())
        .process(move |stream: &pw::stream::Stream, _| {
            let mut st = match state_for_process.try_borrow_mut() {
                Ok(st) => st,
                Err(_) => return, // Another callback holds the borrow; skip this cycle
            };

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

    let pod_bytes = build_audio_format_params(audio_format, sample_rate, channels)?;
    let pod = Pod::from_bytes(&pod_bytes).ok_or_else(|| anyhow!("Invalid audio format POD"))?;

    stream.connect(
        Direction::Output,
        node_id,
        StreamFlags::AUTOCONNECT | StreamFlags::MAP_BUFFERS | StreamFlags::RT_PROCESS,
        &mut [pod],
    ).context("pw: stream_connect")?;

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
