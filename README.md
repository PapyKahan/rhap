# Rhap - Rust Handcrafted Audio Player

## Summary

Rhap (Rust Handcrafted Audio Player) is a terminal-based music player written in Rust focused on high-quality, lossless audio playback. It supports Windows (WASAPI) and Linux (ALSA, PipeWire) with bitperfect output capability, common audio formats via the Symphonia library, and a terminal interface built with Ratatui.

Key features include:
- TUI-based music player with playlist management
- Bitperfect audio playback with support for various bit depths (16/24/32) and sample rates (up to 768kHz)
- Multiple audio backends: WASAPI (Windows), ALSA and PipeWire (Linux)
- Audio device selection and capability probing (including USB DAC descriptor reading)
- Gapless playback
- Media controls integration (SMTC on Windows, MPRIS/D-Bus on Linux)
- Search functionality in playlists
- Keyboard shortcuts for intuitive control
- Resampling capabilities to match device requirements

## How It Works

### Architecture

Rhap is built with a modular architecture that consists of the following components:

1. **Audio Engine**
   - Trait-based backend abstraction (`HostTrait`, `DeviceTrait`) with enum dispatch
   - **Windows**: WASAPI with exclusive and shared mode support
   - **Linux (ALSA)**: Direct `hw:` device access for guaranteed bitperfect output
   - **Linux (PipeWire)**: Desktop-friendly playback with `node.passthrough` for near-bitperfect output
   - Supports various bit depths (16, 24, 32 bits) and sample rates
   - Accurate USB DAC capability probing via USB stream descriptors

2. **Music Track Management**
   - Parses and decodes various audio formats using the Symphonia library
   - Manages metadata extraction and playback state

3. **Player Core**
   - Coordinates audio decoding and streaming via lock-free ring buffer
   - Handles playback controls (play, pause, stop, next, previous)
   - Manages audio resampling when device and audio file sample rates differ

4. **User Interface**
   - Terminal-based UI built with Ratatui and Crossterm
   - Multiple screens: playlist view, device selector, search widget
   - Keyboard-driven interface with vim-like navigation

### User Interface

The UI is divided into several components:
- Main playlist screen showing audio tracks
- Audio device selector for choosing output devices
- Search widget for finding tracks in the playlist
- Status information showing playback progress and track details

### Keyboard Controls

- `p` - Play selected track
- `Space` - Pause/Resume playback
- `s` - Stop playback
- `l` - Next track
- `h` - Previous track
- `j/Down` - Navigate down in playlist
- `k/Up` - Navigate up in playlist
- `Enter` - Select/Play item
- `q` - Quit application
- `o` - Open device selector
- `/` - Open search
- `Ctrl+n` - Find next match
- `Ctrl+p` - Find previous match

## Audio Backends

### Windows: WASAPI

WASAPI is the default and only backend on Windows. It supports both exclusive mode (bitperfect, direct hardware access) and shared mode.

### Linux: ALSA (Direct Hardware)

ALSA opens `hw:X,Y` devices directly, bypassing PipeWire/PulseAudio and any software mixer. This provides **guaranteed bitperfect** output — samples reach the DAC with zero processing.

**Trade-offs:**
- Other applications cannot use the audio device while rhap is playing
- You may need to stop PipeWire first: `systemctl --user stop pipewire pipewire-pulse wireplumber`
- Requires appropriate permissions for direct hardware access

**Usage:**
```
rhap --backend alsa -p ~/Music/ -d 5
```

### Linux: PipeWire (Desktop-Friendly)

PipeWire is the default backend on Linux when a PipeWire daemon is detected. It integrates with the desktop audio stack while providing near-bitperfect output through several stream properties:

- **`node.passthrough=true`** — bypasses PipeWire's format conversion and per-stream volume processing
- **`node.exclusive=true`** — tells WirePlumber to cork (pause) all other streams on the sink, preventing mixing
- **`node.force-rate`** — requests PipeWire to switch the graph clock to the stream's native sample rate

With these properties and proper configuration (see below), the audio path through PipeWire is: raw samples from rhap → PipeWire graph (no conversion, no mixing) → ALSA sink → DAC.

The only remaining processing is the **sink-level volume**. When it's at 100% (unity gain), the output is bitperfect. Control volume on your DAC/amplifier instead of software for best results.

**Usage:**
```
rhap --backend pipewire -p ~/Music/
```

### Bitperfect Comparison

| Condition | PipeWire | ALSA hw: |
|-----------|----------|----------|
| Rate matches hardware | bitperfect | bitperfect |
| Rate exceeds hardware | resampled | resampled |
| Sink volume at 100% | bitperfect | N/A (no software volume) |
| Sink volume not 100% | modified | N/A |
| Other streams | corked (exclusive) | other apps blocked |

### Configuring PipeWire for Optimal Playback

By default, PipeWire runs its graph at a fixed sample rate (usually 48kHz) and resamples all streams to match. For lossless playback, you need to enable sample rate switching:

**1. Allow sample rate switching**

Create (or edit) the PipeWire configuration drop-in:

```bash
mkdir -p ~/.config/pipewire/pipewire.conf.d

cat > ~/.config/pipewire/pipewire.conf.d/99-lossless.conf << 'EOF'
context.properties = {
    default.clock.allowed-rates = [ 44100 48000 88200 96000 176400 192000 ]
}
EOF
```

This tells PipeWire which sample rates it may switch to. When rhap starts playing a 44.1kHz file, PipeWire will switch the entire graph (and the hardware) to 44.1kHz instead of resampling to 48kHz.

**2. Restart PipeWire**

```bash
systemctl --user restart pipewire
```

**3. Verify with `pw-top`**

While playing a track, run `pw-top` and check that both the rhap stream and the sink show the **same sample rate**:

```
R   93    512  44100 ...    S24LE 2 44100 alsa_output.usb-...
R   86      0  44100 ...    S24LE 2 44100  + rhap
```

If the sink still shows a different rate (e.g., 48000), either:
- The hardware doesn't support that rate (check your DAC's specs)
- The `allowed-rates` config isn't loaded (check with `pw-dump | grep allowed`)

**4. Optional: Set volume to 100% for true bitperfect**

PipeWire applies software volume by default. At any volume other than 100%, samples are modified (float conversion + gain). For true bitperfect, set volume to unity and control volume on your DAC/amplifier:

```bash
wpctl set-volume @DEFAULT_AUDIO_SINK@ 1.0
```

## Build and Installation

### Prerequisites

- Rust and Cargo (latest stable version)
- **Windows**: No additional dependencies
- **Linux**: Development headers for audio backends:
  ```bash
  # Arch Linux
  sudo pacman -S alsa-lib pipewire

  # Debian/Ubuntu
  sudo apt install libasound2-dev libpipewire-0.3-dev

  # Fedora
  sudo dnf install alsa-lib-devel pipewire-devel
  ```

### Building from Source

1. Clone the repository:
   ```
   git clone https://github.com/PapyKahan/rhap.git
   cd rhap
   ```

2. Build the application:
   ```bash
   # Native build (Linux or Windows)
   cargo build --release

   # Cross-compile for Windows from Linux (WSL)
   cargo build --target x86_64-pc-windows-gnu --release
   ```

3. The compiled binary will be available at:
   - Linux: `target/release/rhap`
   - Windows: `target/release/rhap.exe`
   - Cross-compiled: `target/x86_64-pc-windows-gnu/release/rhap.exe`

### Running Rhap

Basic usage:
```
rhap --path <MUSIC_DIRECTORY_OR_FILE>
```

List available audio devices:
```
rhap --list --path <MUSIC_DIRECTORY_OR_FILE>
```

Select a specific audio device:
```
rhap --device <DEVICE_ID> --path <MUSIC_DIRECTORY_OR_FILE>
```

Enable high priority mode for better performance (may require elevated privileges):
```
rhap --high-priority-mode --path <MUSIC_DIRECTORY_OR_FILE>
```

### Command Line Options

- `--path` or `-p`: Path to a music file or directory (required)
- `--list` or `-l`: List available audio devices
- `--device` or `-d`: Specify the audio device ID to use
- `--backend`: Select audio backend (`alsa`, `pipewire`, `wasapi`). Auto-detected by default.
- `--high-priority-mode` or `-H`: Enable high priority mode (SCHED_FIFO on Linux, Pro Audio on Windows)
- `--gapless`: Enable gapless playback between tracks
- `--resample`: Allow resampling when device doesn't support native format
- `--pollmode`: Enable polling mode for audio streaming
- `--help`: Show help information
- `--version`: Show version information

## Dependencies

Rhap relies on the following main libraries:
- [`symphonia`](https://github.com/pdeljanov/symphonia): Audio decoding and format support
- [`ratatui`](https://github.com/ratatui-org/ratatui): Terminal UI framework
- [`crossterm`](https://github.com/crossterm-rs/crossterm): Terminal control and event handling
- [`rubato`](https://github.com/HEnquist/rubato): Audio resampling
- [`clap`](https://github.com/clap-rs/clap): Command line argument parsing
- [`windows-rs`](https://github.com/microsoft/windows-rs): Windows API bindings (Windows only)
- [`alsa`](https://crates.io/crates/alsa): ALSA bindings (Linux only)
- [`pipewire`](https://crates.io/crates/pipewire): PipeWire bindings (Linux only)
- [`souvlaki`](https://github.com/Sinono3/souvlaki): Media controls (SMTC/MPRIS)
