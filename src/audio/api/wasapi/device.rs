use windows::core::PCWSTR;
use super::enumerate_devices;

pub struct Device {
    pub id: PCWSTR,
    pub index: u16,
    pub name: String,
}

impl Device {
    pub fn get_device(id: u16) -> Result<PCWSTR, String> {
        let mut selected_device: PCWSTR = PCWSTR(std::ptr::null_mut());
    
        let devices = match enumerate_devices() {
            Ok(devices) => devices,
            Err(err) => {
                println!("Error enumerating devices: {}", err);
                return Err(err);
            }
        };
    
        for dev in devices {
            if dev.index == id {
                selected_device = dev.id;
                break;
            }
        }
    
        if selected_device.is_null() {
            println!("Device not found");
            return Err("Device not found".to_string());
        }
    
        Ok(selected_device)
    }
    pub fn new(inner_device_id: PCWSTR, index: u16, name: String) -> Device {
        let this = Self {
            id: inner_device_id,
            index,
            name,
        };

        this
    }
}
