//! 麦克风设备选择 — 启动时列出设备，支持环境变量/交互选择

use cpal::traits::{DeviceTrait, HostTrait};
use log::info;

#[derive(Debug, Clone)]
pub struct AudioDevice {
    pub id: usize,
    pub name: String,
    pub channels: u16,
    pub sample_rate: u32,
    pub is_default: bool,
}

/// 列出所有可用输入设备到日志
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
                    let d = AudioDevice {
                        id: i,
                        name: name.clone(),
                        channels: cfg.channels(),
                        sample_rate: cfg.sample_rate().0,
                        is_default,
                    };
                    info!("  [{}] {} ({}ch {}Hz){}",
                        i, d.name, d.channels, d.sample_rate,
                        if d.is_default { " [默认]" } else { "" });
                    devices.push(d);
                }
                Err(_) => {
                    info!("  [{}] {} (配置不可用)", i, name);
                }
            }
        }
    }
    devices
}

/// 根据环境变量 AUDIO_INPUT_DEVICE_ID 选择设备
/// -1 = 默认设备
pub fn resolve_device_id() -> i32 {
    std::env::var("AUDIO_INPUT_DEVICE_ID")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(-1)
}

pub fn device_name(device_id: i32) -> String {
    if device_id < 0 { return "系统默认".to_string(); }
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
