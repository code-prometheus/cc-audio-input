//! 麦克风设备选择

use cpal::traits::{DeviceTrait, HostTrait};
use log::info;

/// 音频设备信息
#[derive(Debug, Clone)]
pub struct AudioDevice {
    pub id: usize,
    pub name: String,
    pub is_default: bool,
}

/// 列出所有可用输入设备
pub fn list_input_devices() -> Vec<AudioDevice> {
    let host = cpal::default_host();
    let default_name = host.default_input_device()
        .map(|d| d.name().unwrap_or_default())
        .unwrap_or_default();

    let mut devices = Vec::new();
    if let Ok(inputs) = host.input_devices() {
        for (i, dev) in inputs.enumerate() {
            let name = dev.name().unwrap_or_else(|_| format!("Device {}", i));
            let is_default = name == default_name;
            match dev.default_input_config() {
                Ok(cfg) => {
                    info!("  [{}] {} ({}ch {}Hz){}",
                        i, name, cfg.channels(),
                        cfg.sample_rate().0,
                        if is_default { " [默认]" } else { "" });
                    devices.push(AudioDevice { id: i, name, is_default });
                }
                Err(_) => {
                    info!("  [{}] {} (配置不可用)", i, name);
                }
            }
        }
    }
    devices
}

/// 获取指定设备的名称
pub fn device_name(device_id: i32) -> String {
    if device_id < 0 {
        return "系统默认".to_string();
    }
    let host = cpal::default_host();
    if let Ok(devices) = host.input_devices() {
        for (i, dev) in devices.enumerate() {
            if i == device_id as usize {
                return dev.name().unwrap_or_else(|_| format!("Device {}", i));
            }
        }
    }
    format!("Device {}", device_id)
}
