//! 音频设备选择 — 输入 + 输出设备交互式选择

use cpal::traits::{DeviceTrait, HostTrait};
use std::io::Write;

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

/// 解析输入设备:
/// 1. 环境变量 AUDIO_INPUT_DEVICE_ID (精确索引)
/// 2. 名称模糊匹配 "EDIFIER" / "耳机"
/// 3. 只有1个设备 → 自动选它
/// 4. 匹配不到且有多个设备 → 弹出交互菜单让用户选
pub fn resolve_input_device() -> i32 {
    // 环境变量最高优先级
    if let Ok(id) = std::env::var("AUDIO_INPUT_DEVICE_ID") {
        if let Ok(n) = id.parse::<i32>() { return n; }
    }

    let devices = list_devices(true);
    log::info!("🔍 检测到 {} 个输入设备:", devices.len());
    for d in &devices {
        log::info!("   [{}] {} ({}ch {}Hz) default={}", d.id, d.name, d.channels, d.sample_rate, d.is_default);
    }
    if devices.is_empty() { return -1; }

    // 模糊匹配: 包含 "EDIFIER" 或 "耳机" 的设备
    for d in &devices {
        if d.name.to_lowercase().contains("edifier") || d.name.contains("耳机") {
            log::info!("🎧 自动选择耳机: {} (id={})", d.name, d.id);
            return d.id as i32;
        }
    }

    // 仅1个设备 → 自动选
    if devices.len() == 1 {
        log::info!("🎧 仅一个设备, 自动选择: {} (id={})", devices[0].name, devices[0].id);
        return devices[0].id as i32;
    }

    // 多个设备且匹配不到 → 弹出菜单
    interactive_menu("输入", &devices)
}

/// 交互式设备选择菜单 (仅在智能匹配失败时弹出)
fn interactive_menu(label: &str, devices: &[AudioDevice]) -> i32 {
    println!();
    println!("╔══════════════════════════════════════╗");
    println!("║  🎧 未能自动识别耳机, 请手动选择{}设备 ║", label);
    println!("╠══════════════════════════════════════╣");
    for d in devices {
        let mark = if d.is_default { " ⭐默认" } else { "" };
        let name_short = if d.name.len() > 36 { format!("{}...", &d.name[..33]) } else { d.name.clone() };
        println!("║  [{}] {} ({}ch {}Hz){}", d.id, name_short, d.channels, d.sample_rate, mark);
    }
    println!("║  [D] 使用系统默认                   ║");
    println!("╚══════════════════════════════════════╝");
    print!("  输入编号或 D → ");
    let _ = std::io::stdout().flush();

    let mut input = String::new();
    if std::io::stdin().read_line(&mut input).is_ok() {
        let t = input.trim();
        if t.eq_ignore_ascii_case("d") || t.is_empty() { println!("  ✅ 系统默认\r\n"); return -1; }
        if let Ok(n) = t.parse::<i32>() {
            if n >= 0 && (n as usize) < devices.len() {
                println!("  ✅ {}\r\n", devices[n as usize].name);
                return n;
            }
        }
    }
    println!("  ⚠️ 无效输入, 使用系统默认\r\n");
    -1
}
pub fn resolve_output_device() -> i32 {
    let devices = list_devices(false);
    if devices.len() <= 1 { return if devices.is_empty() { -1 } else { devices[0].id as i32 }; }
    interactive_menu("输出", &devices)
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
