# CLAUDE.md

## Project

rhap - A terminal-based audio player for Windows using WASAPI, built with Rust.

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

- The app uses WASAPI via the `windows` crate, so it must be built as a Windows binary (not native Linux).
- The `x86_64-pc-windows-gnu` target is used (not `msvc`) since MSVC toolchain is not available on Linux.
- Rust must be installed via `rustup` (not the distro `rust` package) to get the Windows target's standard library.
