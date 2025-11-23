# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Development Commands

### Building and Running
```bash
# Standard debug build
cargo build

# Optimized release build (uses LTO, fat codegen, strip symbols)
cargo build --release

# Run with music file/directory
cargo run -- --path <MUSIC_DIRECTORY_OR_FILE>

# List available audio devices
cargo run -- --list --path <MUSIC_DIRECTORY_OR_FILE>

# Use specific audio device
cargo run -- --device <DEVICE_ID> --path <MUSIC_DIRECTORY_OR_FILE>

# Enable high priority mode (better performance, may need elevated privileges)
cargo run -- --high-priority-mode --path <MUSIC_DIRECTORY_OR_FILE>
```

### Testing and Development
```bash
# Run tests
cargo test

# Run a specific test
cargo test <test_name>

# Check code without building
cargo check

# Format code
cargo fmt

# Run clippy lints
cargo clippy
```

## Architecture Overview

Rhap is a terminal-based music player with a modular architecture built around four main layers:

### Core Modules
- **`main.rs`**: CLI argument parsing via clap, application entry point, async main using tokio
- **`player.rs`**: Audio player engine handling playback controls, streaming, and resampling
- **`musictrack.rs`**: Audio file parsing and metadata extraction using Symphonia
- **`tools/mod.rs`**: Utility modules including resampling (uses Rubato)

### Audio System (`src/audio/`)
The audio layer abstracts WASAPI functionality through traits:
- **`mod.rs`**: Core audio traits and types (`Audio`, `AudioProcessor`, `HostTrait`, `DeviceTrait`)
- **`host.rs`**: Audio host management for WASAPI implementation
- **`device.rs`**: Audio device handling with capability detection
- **`api/`**: Low-level WASAPI implementation details

### User Interface (`src/ui/`)
Terminal UI built with Ratatui using an event-driven model:
- **`app.rs`**: Main application controller, orchestrates UI state and event handling
- **`keyboard_manager.rs`**: Processes keyboard input with vim-like navigation
- **`screens/playlist.rs`**: Main playlist interface with track selection
- **`widgets/`**: Reusable UI components:
  - `device_selector.rs`: Audio device selection modal
  - `search_widget.rs`: Real-time search functionality
  - `currently_playing_widget.rs`: Now playing display

## Key Technical Details

### Audio Pipeline
1. **File Decoding**: Symphonia handles multi-format audio (MP3, FLAC, WAV, etc.)
2. **Resampling**: Rubato provides high-quality sample rate conversion when device/file specs differ
3. **WASAPI Integration**: Direct Windows audio output through traits abstraction
4. **Async Streaming**: Tokio manages audio streaming for non-blocking playback

### UI Architecture
- Event-driven terminal UI using crossterm for keyboard events
- Modal dialogs for device selection and search
- Real-time updates for playback progress
- Vim-like keyboard navigation (h/j/k/l for movement, space for pause, etc.)

### Performance Considerations
- Release build optimized with LTO, single codegen unit, and stripped symbols
- High-priority mode option for better audio performance
- Parallel processing via Rayon where applicable
- SIMD optimizations in Symphonia for faster decoding

## Platform Requirements
- **Windows-only**: Uses WASAPI (Windows Audio Session API) for audio output
- **Build Tools**: Requires Visual Studio Build Tools with C++ development tools
- **Rust**: Latest stable version recommended

## Key Dependencies
- `symphonia`: Multi-format audio decoding with SIMD optimizations
- `ratatui`: Terminal UI framework with crossterm backend
- `rubato`: High-quality audio resampling with FFT support
- `tokio`: Async runtime for audio streaming
- `windows-rs`: Windows API bindings for WASAPI integration
- `clap`: CLI argument parsing with derive features