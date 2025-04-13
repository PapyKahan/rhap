# Rhap - Rust Handcrafted Audio Player

## Summary

Rhap (Rust Handcrafted Audio Player) is a simple terminal-based music player written in Rust. It uses WASAPI (Windows Audio Session API) for audio playback, supports common audio formats via the Symphonia library, and provides a basic terminal interface built with Ratatui.

Key features include:
- TUI-based music player with playlist management
- High-quality audio playback with support for various bit depths and sample rates
- Audio device selection for playback
- Search functionality in playlists
- Keyboard shortcuts for intuitive control
- Resampling capabilities to match device requirements

## How It Works

### Architecture

Rhap is built with a modular architecture that consists of the following components:

1. **Audio Engine**
   - Handles low-level audio playback through WASAPI (Windows Audio Session API)
   - Supports various bit depths (16, 24, 32 bits) and sample rates
   - Provides audio device enumeration and selection

2. **Music Track Management**
   - Parses and decodes various audio formats using the Symphonia library
   - Manages metadata extraction and playback state

3. **Player Core**
   - Coordinates audio decoding and streaming
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
- `j/↓` - Navigate down in playlist
- `k/↑` - Navigate up in playlist
- `Enter` - Select/Play item
- `q` - Quit application
- `o` - Open device selector
- `/` - Open search
- `Ctrl+n` - Find next match
- `Ctrl+p` - Find previous match

## Build and Installation

### Prerequisites

- Rust and Cargo (latest stable version)
- For Windows: Visual Studio Build Tools with C++ development tools

### Building from Source

1. Clone the repository:
   ```
   git clone https://github.com/yourusername/rhap.git
   cd rhap
   ```

2. Build the application:
   ```
   cargo build --release
   ```

3. The compiled binary will be available at `target/release/rhap.exe`

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

- `--path` or `-p`: Path to a music file or directory
- `--list` or `-l`: List available audio devices
- `--device` or `-d`: Specify the audio device ID to use
- `--high-priority-mode` or `-h`: Enable high priority mode for better performance
- `--pollmode`: Enable polling mode for audio streaming
- `--help`: Show help information
- `--version`: Show version information

## Dependencies

Rhap relies on the following main libraries:
- [`symphonia`](https://github.com/pdeljanov/symphonia): Audio decoding and format support
- [`ratatui`](https://github.com/ratatui-org/ratatui): Terminal UI framework
- [`crossterm`](https://github.com/crossterm-rs/crossterm): Terminal control and event handling
- [`rubato`](https://github.com/HEnquist/rubato): Audio resampling
- [`tokio`](https://github.com/tokio-rs/tokio): Asynchronous runtime
- [`clap`](https://github.com/clap-rs/clap): Command line argument parsing
- [`windows-rs`](https://github.com/microsoft/windows-rs): Windows API bindings for audio support
