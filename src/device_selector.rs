//! 音频设备选择 — 输入 + 输出设备交互式选择

use cpal::traits::{DeviceTrait, HostTrait};
use std::io::{self, Write};

#[derive(Debug, Clone)]
pub struct AudioDevice {
    pub id: usize,
    pub name: String,
    pub channels: u16,
    pub sample_rate: u32,
    pub is_default: bool,
}

/// 列出输入设备
pub fn list_input_devices() -> Vec<AudioDevice> { list_devices(true) }

/// 列出输出设备
pub fn list_output_devices() -> Vec<AudioDevice> { list_devices(false) }

fn list_devices(input: bool) -> Vec<AudioDevice> {
    let host = cpal::default_host();
    let default_name = if input {
        host.default_input_device().map(|d| d.name().unwrap_or_default()).unwrap_or_default()
    } else {
        host.default_output_device().map(|d| d.name().unwrap_or_default()).unwrap_or_default()
    };

    let mut devices = Vec::new();
    let iter: Box<dyn Iterator<Item = cpal::Device>> = if input {
        match host.input_devices() { Ok(d) => Box::new(d), Err(_) => return devices }
    } else {
        match host.output_devices() { Ok(d) => Box::new(d), Err(_) => return devices }
    };

    for (i, dev) in iter.enumerate() {
        let name = dev.name().unwrap_or_else(|_| format!("Device {}", i));
        let is_default = name == default_name;
        let cfg_res = if input { dev.default_input_config() } else { dev.default_output_config() };
        if let Ok(cfg) = cfg_res {
            devices.push(AudioDevice {
                id: i,
                name,
                channels: cfg.channels(),
                sample_rate: cfg.sample_rate().0,
                is_default,
            });
        }
    }
    devices
}

/// 交互式选择设备 (先读环境变量，否则打印列表让用户选)
pub fn resolve_input_device() -> i32 { resolve("输入", true) }
pub fn resolve_output_device() -> i32 { resolve("输出", false) }

fn resolve(label: &str, input: bool) -> i32 {
    let env_var = if input { "AUDIO_INPUT_DEVICE_ID" } else { "AUDIO_OUTPUT_DEVICE_ID" };
    if let Ok(id_str) = std::env::var(env_var) {
        if let Ok(id) = id_str.parse::<i32>() { return id; }
    }

    let devices = list_devices(input);
    if devices.len() <= 1 { return if devices.is_empty() { -1 } else { devices[0].id as i32 }; }

    println!();
    println!("╔══════════════════════════════════════╗");
    println!("║  🎧 选择{}设备                      ║", label);
    println!("╠══════════════════════════════════════╣");
    for d in &devices {
        let mark = if d.is_default { " ⭐默认" } else { "" };
        let name_short = if d.name.len() > 36 { format!("{}...", &d.name[..33]) } else { d.name.clone() };
        println!("║  [{}] {} ({}ch {}Hz){}", d.id, name_short, d.channels, d.sample_rate, mark);
    }
    println!("║  [D] 使用系统默认                   ║");
    println!("╚══════════════════════════════════════╝");
    print!("  {}设备编号或 D → ", label);
    let _ = io::stdout().flush();

    let mut user_input = String::new();
    if io::stdin().read_line(&mut user_input).is_ok() {
        let t = user_input.trim();
        if t.eq_ignore_ascii_case("d") || t.is_empty() { println!("  ✅ 默认\r\n"); return -1; }
        if let Ok(n) = t.parse::<i32>() {
            if n >= 0 && (n as usize) < devices.len() {
                println!("  ✅ {}\r\n", devices[n as usize].name);
                return n;
            }
        }
    }
    println!("  ⚠️ 无效，用默认\r\n");
    -1
}

/// 获取设备名称
pub fn input_device_name(id: i32) -> String { device_name(id, true) }
pub fn output_device_name(id: i32) -> String { device_name(id, false) }

fn device_name(id: i32, input: bool) -> String {
    if id < 0 { return "系统默认".to_string(); }
    for d in &list_devices(input) {
        if d.id == id as usize { return d.name.clone(); }
    }
    format!("Device {}", id)
}

/// 获取输出设备（cpal Device）
pub fn get_output_device(id: i32) -> Option<cpal::Device> {
    let host = cpal::default_host();
    if id < 0 {
        host.default_output_device().map(Some).unwrap_or(None)
    } else {
        host.output_devices().ok()
            .and_then(|mut devs| devs.nth(id as usize))
    }
}
