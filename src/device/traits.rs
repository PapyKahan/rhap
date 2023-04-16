/// This module contains the traits that are used by the library.
/// The traits are used to abstract the underlying audio library.
/// This allows the library to be used with different audio libraries.
/// The traits are implemented by the audio library specific modules.
pub trait AudioDevice {
    fn get_name(&self) -> String;
    fn get_id(&self) -> String;
    fn is_default(&self) -> bool;
    fn get_capabilities(&self) -> Vec<String>;
}

pub trait HostAudioApi {
    fn get_name(&self) -> String;
    fn get_devices(&self) -> Vec<Box<AudioDevice>>;
    fn get_default_device(&self) -> Box<AudioDevice>;
}
