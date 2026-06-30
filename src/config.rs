//! 配置模块 — LLM参数硬编码 + 可选的环境变量覆盖

use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct AppConfig {
    pub llm: LlmConfig,
    pub hotkey: HotkeyConfig,
    pub audio: AudioConfig,
    pub asr: AsrConfig,
}

#[derive(Debug, Clone)]
pub struct LlmConfig {
    pub base_url: String,
    pub api_key: String,
    pub model: String,
    pub verify_ssl: bool,
}

#[derive(Debug, Clone)]
pub struct HotkeyConfig {
    pub hold_ms: u64,
}

#[derive(Debug, Clone)]
pub struct AudioConfig {
    pub device_id: i32,
    pub sample_rate: u32,
    pub channels: u16,
}

#[derive(Debug, Clone)]
pub struct AsrConfig {
    pub model_dir: PathBuf,
}

impl AppConfig {
    pub fn load() -> Self {
        Self {
            llm: LlmConfig {
                base_url: "http://122.1.231.24:8000/v1".to_string(),
                api_key: "none".to_string(),
                model: "dsv4".to_string(),
                verify_ssl: false,
            },
            hotkey: HotkeyConfig {
                hold_ms: env_u64("HOLD_MS", 3000),
            },
            audio: AudioConfig {
                device_id: env_i32("DEVICE_ID", -1),
                sample_rate: 16000,
                channels: 1,
            },
            asr: AsrConfig {
                model_dir: PathBuf::from(
                    std::env::var("MODEL_DIR")
                        .unwrap_or_else(|_| "F:/models/sense-voice-int8".to_string())
                ),
            },
        }
    }
}

fn env_u64(key: &str, default: u64) -> u64 {
    std::env::var(format!("AUDIO_INPUT_{}", key))
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(default)
}

fn env_i32(key: &str, default: i32) -> i32 {
    std::env::var(format!("AUDIO_INPUT_{}", key))
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(default)
}
