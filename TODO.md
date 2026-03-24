# Code Review Findings

Comprehensive review of the rhap codebase — Rust best practices, performance, memory usage, and correctness.

## Critical

- [ ] **ALSA short write discards frames** (`alsa/api.rs:112`) — `writei` returning fewer frames than requested is normal; the tail is silently dropped instead of retried. Audio frames are permanently lost on every short write.
- [ ] **ALSA `nice()` check is wrong** (`alsa/api.rs:315`) — `nice()` can legitimately return -1 on success. Must clear `errno` before the call and check it after, not check the return value.
- [ ] **PipeWire `unsafe impl Sync` is unsound** (`pipewire/device.rs:42`) — `Device` has no interior synchronization. Two threads could call `start()`/`stop()` concurrently through `&Device`. The trait requires `Sync` but the impl is unsafe without a `Mutex`.
- [ ] **Integer underflow panic on empty playlist** (`app_state.rs:322`) — `len - 1` underflows for `usize` when `playlist.songs_len() == 0`. Same in `previous()`.
- [ ] **Stop ordering race** (`player.rs:217`) — `stop()` joins the decoder thread, then calls `device.stop()`. The audio output thread is still consuming the ring buffer and calling into the driver while the device is being torn down.

## Major

- [ ] **ALSA pause state machine race** (`alsa/device.rs:97`) — `is_paused` is both a command and a state. Rapid pause/unpause can call `pcm.pause()`/`pcm.resume()` in unexpected sequences. Needs a command/acknowledgment or state machine pattern.
- [ ] **Heap alloc in ALSA audio thread EOS path** (`alsa/device.rs:121`) — `vec![0u8; remaining]` can be several MB for high-res audio. Should drain iteratively using the pre-allocated buffer.
- [ ] **XRUN in `get_writable_bytes` is fatal** (`alsa/api.rs:158`) — `avail_update()` error kills the thread. The `write()` method recovers from XRUNs; this path doesn't.
- [ ] **PipeWire `expect()` panic in spawned thread** (`pipewire/api.rs:131`) — Serialization failure panics the audio thread, silently swallowed as "thread panicked". Should return `Result`.
- [ ] **PipeWire ALSA device 0 hardcoded** (`pipewire/host.rs:42`) — `object.path` contains the device number in `parts[3]` but it's discarded. Multi-device cards probe the wrong device.
- [ ] **PipeWire default device is first enumerated** (`pipewire/host.rs:139`) — Not PipeWire's actual default. Should consult the `default.audio.sink` metadata.
- [ ] **File probed twice per play** (`musictrack.rs:53,128`) — `new()` probes metadata, `open_for_playback()` re-probes from scratch. Double I/O for every `play()` call.
- [ ] **FFT resampler rebuilt on every frame-count change** (`player.rs`, resampler) — Last packet of FLAC/MP3 tracks is shorter, triggering a full FFT plan reallocation per track end.
- [ ] **`auto_advance` retries all tracks in one UI tick** (`app_state.rs:79`) — If every track fails to open, blocks the UI for seconds doing `len` stop/play cycles.
- [ ] **PipeWire detection unreliable** (`host.rs:75`) — Empty `XDG_RUNTIME_DIR` produces path `/pipewire-0`. `PIPEWIRE_REMOTE` overrides are missed.

## Medium

- [ ] **Elapsed time overcounts on stop** (`player.rs:334`) — Incremented before packet is decoded/written to the ring buffer.
- [ ] **Byte packing loop not vectorizable** (`player.rs:170`) — `extend_from_slice` called per sample (384k calls/sec at 192kHz stereo). Transmute-based bulk conversion or pre-sized reserve would help.
- [ ] **`write_all_blocking` busy-polls at 5ms** (`player.rs:182`) — Mutex acquire on every iteration. Chatty wakeup pattern with unnecessary OS scheduler pressure.
- [ ] **Double-atomic `is_playing` + `is_paused`** (`player.rs:258`) — Not atomically composable. A single `AtomicU8` state machine would be cleaner and cheaper.
- [ ] **PipeWire `RefCell::borrow_mut()` panic risk** (`pipewire/api.rs:189`) — Process callback panics on re-entrant access. `try_borrow_mut()` with early return would be safer.
- [ ] **PipeWire `node.passthrough` unconditional** (`pipewire/api.rs:166`) — May cause connection failure on Bluetooth sinks with no fallback.
- [ ] **32-bit mapped to F32LE only** (`pipewire/api.rs:105`) — `S32LE` integer content would be misinterpreted as float noise.
- [ ] **ALSA period time hardcoded at 5ms** (`alsa/api.rs:58`) — No validation against actual negotiated value. Some USB/virtual devices can't sustain this.
- [ ] **Panic on malformed files** (`musictrack.rs:65,139`) — `.get(0).unwrap()` panics on empty track list.
- [ ] **Cover art double allocation** (`musictrack.rs:108`) — Bytes copied into `Arc<[u8]>` from Symphonia's buffer; large album art (~1-5 MB) exists twice in memory.
- [ ] **Previous-track restart uses decoder time** (`app_state.rs:331`) — Elapsed time is ahead of audible position due to buffering, causing false "restart" triggers.
- [ ] **Log file `expect()` panics before UI setup** (`main.rs:64`) — Terminal left in inconsistent state if temp dir is unavailable.

## Low

- [ ] **`ThreadPriority` struct has no `Drop`** (`alsa/api.rs:301`) — Falsely implies RAII cleanup of scheduler policy. Should be a plain function.
- [ ] **`"hw:0,0"` hardcoded as default** (`alsa/host.rs:20`) — Bypasses user ALSA config (`.asoundrc`, UCM).
- [ ] **Capability probe error silently discarded** (`alsa/device.rs:53`) — Should log at warn level.
- [ ] **`adjust_with_capabilities` panics on empty vecs** (`audio/mod.rs:80`) — `.last().unwrap()` on empty `sample_rates` or `bits_per_samples`.
- [ ] **PipeWire `done` Rc is dead code** (`pipewire/host.rs:54`) — Set but never read after `main_loop.run()`.
- [ ] **PipeWire `_node_name` dropped immediately** (`pipewire/device.rs:23`) — Wasted allocation.
- [ ] **Redundant `MetadataRevision::default().clone()`** (`musictrack.rs:77`)
- [ ] **Unnecessary `path.clone()` for `File::open`** (`musictrack.rs:54,128`)
- [ ] **`index = index + 1`** (`main.rs:95`) — Should be `index += 1`.
- [ ] **`"auto".to_string()` allocates unnecessarily** (`main.rs:73`) — `&str` would suffice.
