#[cfg(target_os = "linux")]
use anyhow::{Result, anyhow};
#[cfg(target_os = "linux")]
use crate::audio::{Device, HostTrait};
#[cfg(target_os = "linux")]
use super::device::Device as AlsaDevice;

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

    fn enumerate_alsa_devices() -> Result<Vec<(String, bool)>> {
        let mut devices = Vec::new();

        // Add the default device
        devices.push(("default".to_string(), true));

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
                                            devices.push((device_name, false));
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
            "pulse", // PulseAudio over ALSA
            "plughw:0,0",
            "hw:0,0",
            "default",
        ];

        for device in common_devices {
            if !devices.iter().any(|(name, _)| name == device) {
                // Try to open the device to see if it exists
                if let Ok(device_cstr) = std::ffi::CString::new(device) {
                    if alsa::pcm::PCM::open(&device_cstr, alsa::Direction::Playback, false).is_ok() {
                        devices.push((device.to_string(), false));
                    }
                }
            }
        }

        Ok(devices)
    }
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
                .find(|(_, is_default)| *is_default)
                .or_else(|| devices.first())
                .ok_or_else(|| anyhow!("No audio devices found"))?
        };

        let alsa_device = AlsaDevice::new(device_info.0.clone(), device_info.1);
        Ok(Device::Alsa(alsa_device))
    }

    fn get_devices(&self) -> Result<Vec<Device>> {
        let devices_info = Self::enumerate_alsa_devices()?;
        let mut devices = Vec::new();

        for (name, is_default) in devices_info {
            let alsa_device = AlsaDevice::new(name, is_default);
            devices.push(Device::Alsa(alsa_device));
        }

        Ok(devices)
    }

    fn get_default_device(&self) -> Result<Device> {
        let devices = Self::enumerate_alsa_devices()?;

        let default_device = devices.iter()
            .find(|(_, is_default)| *is_default)
            .or_else(|| devices.first())
            .ok_or_else(|| anyhow!("No default audio device found"))?;

        let alsa_device = AlsaDevice::new(default_device.0.clone(), default_device.1);
        Ok(Device::Alsa(alsa_device))
    }
}