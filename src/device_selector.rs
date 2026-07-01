//! 麦克风设备选择 — 启动时交互式选择

use cpal::traits::{DeviceTrait, HostTrait};
use log::info;
use std::io::{self, Write};

#[derive(Debug, Clone)]
pub struct AudioDevice {
    pub id: usize,
    pub name: String,
    pub channels: u16,
    pub sample_rate: u32,
    pub is_default: bool,
}

/// 列出所有可用输入设备，返回列表
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
                    devices.push(AudioDevice {
                        id: i,
                        name,
                        channels: cfg.channels(),
                        sample_rate: cfg.sample_rate().0,
                        is_default,
                    });
                }
                Err(_) => {}
            }
        }
    }
    devices
}

/// 交互式选择设备（先读环境变量，否则打印列表让用户输入）
pub fn resolve_device_id() -> i32 {
    // 优先环境变量
    if let Ok(id_str) = std::env::var("AUDIO_INPUT_DEVICE_ID") {
        if let Ok(id) = id_str.parse::<i32>() {
            return id;
        }
    }

    let devices = list_input_devices();

    // 只有一个设备或没有设备，直接用默认
    if devices.len() <= 1 {
        return -1;
    }

    // 打印设备列表并等待输入
    println!();
    println!("╔══════════════════════════════════════╗");
    println!("║  🎤 选择输入设备                     ║");
    println!("╠══════════════════════════════════════╣");
    for d in &devices {
        let mark = if d.is_default { " ⭐默认" } else { "" };
        println!("║  [{}] {} ({}ch {}Hz){}", d.id, d.name, d.channels, d.sample_rate, mark);
    }
    println!("║  [D] 使用系统默认设备               ║");
    println!("╚══════════════════════════════════════╝");
    print!("  输入编号或 D → ");
    let _ = io::stdout().flush();

    let mut input = String::new();
    if io::stdin().read_line(&mut input).is_ok() {
        let trimmed = input.trim();
        if trimmed.eq_ignore_ascii_case("d") || trimmed.is_empty() {
            println!("  ✅ 已选: 系统默认\r\n");
            return -1;
        }
        if let Ok(n) = trimmed.parse::<i32>() {
            if n >= 0 && (n as usize) < devices.len() {
                println!("  ✅ 已选: {}\r\n", devices[n as usize].name);
                return n;
            }
        }
    }

    // 无效输入 → 默认
    println!("  ⚠️ 无效输入，使用默认\r\n");
    -1
}

pub fn device_name(device_id: i32) -> String {
    if device_id < 0 { return "系统默认".to_string(); }
    let devices = list_input_devices();
    for d in &devices {
        if d.id == device_id as usize {
            return d.name.clone();
        }
    }
    format!("Device {}", device_id)
}
