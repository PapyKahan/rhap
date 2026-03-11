## Project

rhap - A terminal-based audio player for Windows (WASAPI) and Linux (ALSA), built with Rust.

## Cross-building for Windows (amd64) from WSL

### Prerequisites

Install on Arch Linux (WSL):

```sh
sudo pacman -S mingw-w64-gcc rustup
rustup target add x86_64-pc-windows-gnu
```

### Build

```sh
# Debug
cargo build --target x86_64-pc-windows-gnu

# Release
cargo build --target x86_64-pc-windows-gnu --release
```

### Output

- Debug: `target/x86_64-pc-windows-gnu/debug/rhap.exe`
- Release: `target/x86_64-pc-windows-gnu/release/rhap.exe`

### Notes

- The app uses WASAPI via the `windows` crate for Windows, and ALSA via the `alsa` crate for Linux.
- When building for Windows on Linux, the `x86_64-pc-windows-gnu` target is used.
- Rust must be installed via `rustup` to get the Windows target's standard library.

## Development Commands

### Building and Running (Native)
```bash
# Standard debug build
cargo build

# Optimized release build
cargo build --release

# Run with music file/directory
cargo run -- --path <MUSIC_DIRECTORY_OR_FILE>

# List available audio devices
cargo run -- --list --path <MUSIC_DIRECTORY_OR_FILE>
```

### Testing and Development
```bash
cargo test
cargo fmt
cargo clippy
```

## Architecture Overview

- **`main.rs`**: Entry point and CLI parsing.
- **`player.rs`**: Playback engine and streaming.
- **`audio/`**: Platform-agnostic traits with ALSA and WASAPI implementations.
- **`ui/`**: Ratatui-based terminal interface.
- **`tools/resampler.rs`**: Audio resampling using Rubato.
