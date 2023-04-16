use crate::audio_device::AudioDevice;
use crate::host_audio_api::HostAudioApi;

pub struct HostAudioApiManager {
    pub host_audio_apis: Vec<HostAudioApi>
}

impl HostAudioApiManager {
    pub fn new() -> AudioDeviceManager {
        AudioDeviceManager {}
    }

    pub fn get_audio_apis(&self) -> Vec<String> {
        vec!["default".to_string()]
    }

    pub fn get_devices(&self) -> Vec<AudioDevice> {
        let mut devices = Vec::new();
        let device = AudioDevice::new("default", "default", true, vec!["playback".to_string(), "capture".to_string()]);
        devices.push(device);
        devices
    }
}
