use anyhow::Result;

use crate::audio::{Device as AudioDevice, HostTrait};
use super::device::Device;

#[derive(Clone, Copy)]
pub struct Host {
    high_priority_mode: bool,
}

impl Host {
    pub(crate) fn new(high_priority_mode: bool) -> Self {
        Self { high_priority_mode }
    }

    /// Enumerate all ALSA playback PCM devices.
    /// Returns a vec of (hw_device_name, friendly_name, is_default).
    fn enumerate_devices(&self) -> Result<Vec<(String, String, bool)>> {
        let mut devices = Vec::new();
        let default_name = "hw:0,0".to_string();

        for card_result in alsa::card::Iter::new() {
            let card = match card_result {
                Ok(c) => c,
                Err(_) => continue,
            };
            let card_index = card.get_index();
            let card_name = card
                .get_name()
                .unwrap_or_else(|_| format!("Card {}", card_index));

            let ctl_name = format!("hw:{}", card_index);
            let ctl = match alsa::Ctl::new(&ctl_name, false) {
                Ok(c) => c,
                Err(_) => continue,
            };

            for dev_index in alsa::ctl::DeviceIter::new(&ctl) {
                let info = match ctl.pcm_info(dev_index as u32, 0, alsa::Direction::Playback) {
                    Ok(i) => i,
                    Err(_) => continue,
                };

                let device_name = format!("hw:{},{}", card_index, dev_index);
                let pcm_dev_name = info.get_name().unwrap_or("unknown");
                let friendly = format!("{} [{}]", card_name, pcm_dev_name);
                let is_def = device_name == default_name;
                devices.push((device_name, friendly, is_def));
            }
        }

        // Fallback: if no device was found, present a single default entry.
        if devices.is_empty() {
            devices.push((
                default_name,
                "Default ALSA device".to_string(),
                true,
            ));
        }

        Ok(devices)
    }
}

impl HostTrait for Host {
    fn get_devices(&self) -> Result<Vec<AudioDevice>> {
        let raw = self.enumerate_devices()?;
        Ok(raw
            .into_iter()
            .map(|(name, friendly, is_def)| {
                AudioDevice::Alsa(Device::new(name, friendly, is_def, self.high_priority_mode))
            })
            .collect())
    }

    fn create_device(&self, id: Option<u32>) -> Result<AudioDevice> {
        let raw = self.enumerate_devices()?;
        let (name, friendly, is_def) = match id {
            Some(index) => raw
                .into_iter()
                .nth(index as usize)
                .ok_or_else(|| anyhow::anyhow!("Device index {} not found", index))?,
            None => raw
                .into_iter()
                .next()
                .ok_or_else(|| anyhow::anyhow!("No ALSA playback devices found"))?,
        };
        Ok(AudioDevice::Alsa(Device::new(
            name,
            friendly,
            is_def,
            self.high_priority_mode,
        )))
    }

    fn get_default_device(&self) -> Result<AudioDevice> {
        self.create_device(None)
    }
}
