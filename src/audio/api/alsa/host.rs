#[cfg(target_os = "linux")]
use anyhow::{Result, anyhow};
#[cfg(target_os = "linux")]
use crate::audio::{Device, HostTrait};
#[cfg(target_os = "linux")]
use super::device::Device as AlsaDevice;
#[cfg(target_os = "linux")]
use crate::logging::log_to_file_only;
#[cfg(target_os = "linux")]
use alsa::{Ctl, Direction};
#[cfg(target_os = "linux")]
use std::ffi::CString;
#[cfg(target_os = "linux")]
use std::collections::HashMap;

#[cfg(target_os = "linux")]
#[derive(Clone)]
pub struct Host {
    high_priority_mode: bool,
}

#[cfg(target_os = "linux")]
impl Host {
    pub fn new(high_priority_mode: bool) -> Self {
        Self {
            high_priority_mode,
        }
    }

    fn enumerate_alsa_devices() -> Result<Vec<(String, bool, String)>> {
        let mut devices = Vec::new();

        // Add the default device
        devices.push(("default".to_string(), true, "Default Device".to_string()));

        // Try to enumerate ALSA devices using /proc/asound/pcm
        if let Ok(pcm_entries) = std::fs::read_dir("/proc/asound/pcm") {
            for entry in pcm_entries {
                if let Ok(entry) = entry {
                    if let Ok(file_name) = entry.file_name().into_string() {
                        if file_name.ends_with("c") { // Capture devices end with 'c', we want playback
                            continue;
                        }

                        // Parse the PCM info to get device name
                        if let Ok(content) = std::fs::read_to_string(entry.path()) {
                            for line in content.lines() {
                                if line.contains("playback") {
                                    // Extract card number and device number
                                    if let Some(captures) = regex::Regex::new(r"card (\d+): device (\d+)")?.captures(&file_name) {
                                        if let (Some(card_match), Some(device_match)) = (captures.get(1), captures.get(2)) {
                                            let card_num = card_match.as_str();
                                            let device_num = device_match.as_str();
                                            let device_name = format!("hw:{},{}", card_num, device_num);
                                            // Use device name as description for fallback
                                            devices.push((device_name.clone(), false, device_name));
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        // Add common device names if they exist
        let common_devices = vec![
            ("pulse", "PulseAudio"),
            ("plughw:0,0", "Hardware 0,0"),
            ("hw:0,0", "Hardware 0,0"),
            ("default", "Default Device"),
        ];

        for (device, desc) in common_devices {
            if !devices.iter().any(|(name, _, _)| name == device) {
                // Try to open the device to see if it exists
                if let Ok(device_cstr) = std::ffi::CString::new(device) {
                    if alsa::pcm::PCM::open(&device_cstr, alsa::Direction::Playback, false).is_ok() {
                        devices.push((device.to_string(), false, desc.to_string()));
                    }
                }
            }
        }

        Ok(devices)
    }

    // Helper function for fallback enumeration
    fn get_devices_fallback(&self) -> Result<Vec<Device>> {
        let devices_info = Self::enumerate_alsa_devices()?;
        let mut devices = Vec::new();

        for (name, is_default, description) in devices_info {
            let alsa_device = AlsaDevice::new(name, is_default, description);
            devices.push(Device::Alsa(alsa_device));
        }

        Ok(devices)
    }
}

// Native ALSA device enumeration functions
#[cfg(target_os = "linux")]
fn enumerate_alsa_devices_native() -> Result<Vec<(String, bool, String)>> {
    let mut devices = Vec::new();

    // Add the default device
    devices.push(("default".to_string(), true, "Default ALSA device".to_string()));

    // Open control interface
    let ctl = match Ctl::open(&CString::new("default")?, false) {
        Ok(ctl) => ctl,
        Err(e) => {
            log_to_file_only("ALSA", &format!("Failed to open CTL interface: {}", e));
            return Ok(devices); // Return just the default device
        }
    };

    // Try card 0 first
    if let Ok(card_devices) = enumerate_card_devices(&ctl, 0) {
        devices.extend(card_devices);
    }

    // Try card 1 (USB devices)
    if let Ok(card_devices) = enumerate_card_devices(&ctl, 1) {
        devices.extend(card_devices);
    }

    // Add common virtual devices
    let virtual_devices = vec![
        ("pulse", "PulseAudio over ALSA"),
        ("plughw:0,0", "PCM plugin device 0,0"),
        ("plughw:1,0", "PCM plugin device 1,0"),
    ];

    for (name, description) in virtual_devices {
        if !devices.iter().any(|(dev_name, _, _)| dev_name == name) {
            devices.push((name.to_string(), false, description.to_string()));
        }
    }

    // Deduplicate descriptions
    let mut desc_counts = HashMap::new();
    for (_, _, desc) in &devices {
        *desc_counts.entry(desc.clone()).or_insert(0) += 1;
    }

    for (name, _, desc) in &mut devices {
        if let Some(&count) = desc_counts.get(desc) {
            if count > 1 {
                // Append ID to disambiguate
                *desc = format!("{} ({})", desc, name);
            }
        }
    }

    Ok(devices)
}

#[cfg(target_os = "linux")]
fn enumerate_card_devices(ctl: &Ctl, card_num: i32) -> Result<Vec<(String, bool, String)>> {
    let mut devices = Vec::new();

    // Common device numbers to check (0-10 should cover most cases)
    for device_num in 0..=10 {
        // Try to get PCM info for playback direction
        match ctl.pcm_info(device_num as u32, 0, Direction::Playback) {
            Ok(info) => {
                // We found a working playback device
                let device_name = format!("hw:{},{}", card_num, device_num);

                // Get device name and description
                let device_desc = info.get_name()
                    .unwrap_or("Unknown Device")
                    .trim_end_matches("(*)") // Remove (*) if present
                    .to_string();

                devices.push((device_name, false, device_desc.clone()));
                log_to_file_only("ALSA", &format!("Found device: hw:{},{} ({})", card_num, device_num, device_desc));
            }
            Err(_) => {
                // Device doesn't exist or doesn't support playback
                continue;
            }
        }
    }

    Ok(devices)
}

#[cfg(target_os = "linux")]
impl HostTrait for Host {
    fn create_device(&self, id: Option<u32>) -> Result<Device> {
        let devices = Self::enumerate_alsa_devices()?;

        let device_info = if let Some(device_id) = id {
            devices.get(device_id as usize)
                .ok_or_else(|| anyhow!("Device {} not found", device_id))?
        } else {
            // Return the default device
            devices.iter()
                .find(|(_, is_default, _)| *is_default)
                .or_else(|| devices.first())
                .ok_or_else(|| anyhow!("No audio devices found"))?
        };

        let alsa_device = AlsaDevice::new(device_info.0.clone(), device_info.1, device_info.2.clone());
        Ok(Device::Alsa(alsa_device))
    }

    fn get_devices(&self) -> Result<Vec<Device>> {
        // Use native ALSA library enumeration first
        match enumerate_alsa_devices_native() {
            Ok(devices_info) => {
                let mut devices = Vec::new();

                for (name, is_default, description) in devices_info {
                    let alsa_device = AlsaDevice::new(name, is_default, description);
                    devices.push(Device::Alsa(alsa_device));
                }

                Ok(devices)
            }
            Err(e) => {
                log_to_file_only("ALSA", &format!("Native enumeration failed, falling back: {}", e));
                // Fallback to original method if native fails
                self.get_devices_fallback()
            }
        }
    }

    fn get_default_device(&self) -> Result<Device> {
        let devices = Self::enumerate_alsa_devices()?;

        let default_device = devices.iter()
            .find(|(_, is_default, _)| *is_default)
            .or_else(|| devices.first())
            .ok_or_else(|| anyhow!("No default audio device found"))?;

        let alsa_device = AlsaDevice::new(default_device.0.clone(), default_device.1, default_device.2.clone());
        Ok(Device::Alsa(alsa_device))
    }
}