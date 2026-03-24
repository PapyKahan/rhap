#[cfg(target_os = "windows")]
pub(crate) mod wasapi;

#[cfg(target_os = "linux")]
pub(crate) mod pipewire;
