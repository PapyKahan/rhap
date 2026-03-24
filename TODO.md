# Code Review Findings

Comprehensive review of the rhap codebase — Rust best practices, performance, memory usage, and correctness.

## Critical

- [x] **ALSA short write discards frames** (`alsa/api.rs:112`) — Fixed: retry with remaining data offset instead of discarding.
- [x] **ALSA `nice()` check is wrong** (`alsa/api.rs:315`) — Fixed: clear and check errno instead of return value.
- [x] **PipeWire `unsafe impl Sync` is unsound** (`pipewire/device.rs:42`) — Fixed: removed `Sync` bound from `DeviceTrait` and all `unsafe impl Sync` (Device is never shared across threads).
- [x] **Integer underflow panic on empty playlist** (`app_state.rs:322`) — Fixed: guard `len == 0` in both `next()` and `previous()`, use modular arithmetic.
- [x] **Stop ordering race** (`player.rs:217`) — Fixed: stop device (audio output thread) before joining decoder thread.

## Major

- [x] **ALSA pause state machine race** (`alsa/device.rs:97`) — Fixed: separate `hw_paused` bool tracks actual hardware state, only pause/resume when state transitions.
- [x] **Heap alloc in ALSA audio thread EOS path** (`alsa/device.rs:121`) — Fixed: drain iteratively using the pre-allocated `write_buf` instead of allocating.
- [x] **XRUN in `get_writable_bytes` is fatal** (`alsa/api.rs:158`) — Fixed: recover via `try_recover` and re-query, matching the `write()` recovery pattern.
- [x] **PipeWire `expect()` panic in spawned thread** (`pipewire/api.rs:131`) — Fixed: `build_audio_format_params` returns `Result`, propagated to caller.
- [x] **PipeWire ALSA device 0 hardcoded** (`pipewire/host.rs:42`) — Documented: profile_device index in object.path doesn't map to ALSA PCM device number. Device 0 is correct for USB DACs; HDA cards typically share codec capabilities across PCM devices.
- [x] **PipeWire default device is first enumerated** (`pipewire/host.rs:139`) — Fixed: use `priority.session` from global props to identify the highest-priority sink as default.
- [x] **File probed twice per play** (`musictrack.rs:53,128`) — Fixed: added `probe_and_open()` that returns metadata + PlaybackHandle in one probe. Used in `play()` for unprobed tracks.
- [ ] **FFT resampler rebuilt on every frame-count change** (`player.rs`, resampler) — Reverted padding approach (broke accumulator). Rebuild is necessary: rubato FixedSync::Input requires exact frame counts. The `.unwrap()` was replaced with `?`.
- [x] **`auto_advance` retries all tracks in one UI tick** (`app_state.rs:79`) — Fixed: limit retries to 5 to avoid blocking UI.
- [x] **PipeWire detection unreliable** (`host.rs:75`) — Fixed: try actual device enumeration via PipeWire instead of checking socket path.

## Medium

- [x] **Elapsed time overcounts on stop** (`player.rs:334`) — Fixed: move `fetch_add` after `decode()` so elapsed time reflects decoded position.
- [x] **Byte packing loop not vectorizable** (`player.rs:170`) — Fixed: bulk `slice::from_raw_parts` reinterpret instead of per-sample `extend_from_slice`.
- [x] **`write_all_blocking` busy-polls at 5ms** (`player.rs:182`) — Fixed: notify once on full write, notify+wait on partial, wait-only when no progress.
- [ ] **Double-atomic `is_playing` + `is_paused`** (`player.rs:258`) — Deferred: cosmetic. Only composed in UI display path; single-tick stale state is benign.
- [x] **PipeWire `RefCell::borrow_mut()` panic risk** (`pipewire/api.rs:189`) — Fixed: use `try_borrow_mut()` with early return on contention.
- [x] **PipeWire `node.passthrough` unconditional** (`pipewire/api.rs:166`) — Fixed: only set in exclusive mode. Bluetooth sinks won't be rejected.
- [x] **32-bit mapped to F32LE only** (`pipewire/api.rs:105`) — Documented: correct behavior. WASAPI and Symphonia both use IEEE float for 32-bit; ring buffer always contains F32LE.
- [x] **ALSA period time hardcoded at 5ms** (`alsa/api.rs:58`) — Fixed: log actual negotiated period/buffer times, warn when period < 3ms.
- [x] **Panic on malformed files** (`musictrack.rs:65,139`) — Fixed in previous commit via `.first().ok_or_else()`. Also fixed player.rs `tracks().get(0).unwrap()`.
- [ ] **Cover art double allocation** (`musictrack.rs:108`) — Unavoidable: Symphonia's `MetadataRevision::visuals()` returns `&[Visual]` with no consuming API to move `Box<[u8]>` out.
- [x] **Previous-track restart uses decoder time** (`app_state.rs:331`) — Fixed: increased threshold from 3s to 5s to account for buffering lag. Documented the limitation.
- [x] **Log file `expect()` panics before UI setup** (`main.rs:64`) — Fixed: gracefully skip logging if temp file creation fails.

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
