//! 配置模块 — models.yaml + 环境变量

use std::path::PathBuf;
use serde::Deserialize;

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

// ── models.yaml 结构 (单模型) ──
#[derive(Debug, Deserialize)]
struct ModelsFile {
    base_url: String,
    api_key: Option<String>,
    model: String,
    verify_ssl: Option<bool>,
}

impl AppConfig {
    pub fn load() -> Self {
        // 先尝试 settings.json
        if let Some(s) = load_settings_json() {
            return Self {
                llm: s.llm,
                hotkey: s.hotkey,
                audio: AudioConfig {
                    device_id: env_i32("DEVICE_ID", -1),
                    sample_rate: 16000,
                    channels: 1,
                },
                asr: s.asr,
            };
        }

        // 尝试 models.yaml
        let llm = load_llm_config();

        Self {
            llm,
            hotkey: HotkeyConfig {
                hold_ms: env_u64("HOLD_MS", 1500),
            },
            audio: AudioConfig {
                device_id: env_i32("DEVICE_ID", -1),
                sample_rate: 16000,
                channels: 1,
            },
            asr: AsrConfig {
                model_dir: PathBuf::from(
                    std::env::var("MODEL_DIR")
                        .unwrap_or_else(|_| "models/sense-voice-int8".to_string()),
                ),
            },
        }
    }
}

struct LoadedSettings {
    llm: LlmConfig,
    hotkey: HotkeyConfig,
    asr: AsrConfig,
}

fn load_settings_json() -> Option<LoadedSettings> {
    let candidates = [
        PathBuf::from("settings.json"),
        PathBuf::from("assets/settings.json"),
    ];
    for path in &candidates {
        if let Ok(content) = std::fs::read_to_string(path) {
            // 支持新旧两种格式
            if let Ok(sf) = serde_json::from_str::<serde_json::Value>(&content) {
                let llm = LlmConfig {
                    base_url: sf.get("base_url").and_then(|v| v.as_str()).unwrap_or("http://122.1.231.24:8000/v1").to_string(),
                    api_key: sf.get("api_key").and_then(|v| v.as_str()).unwrap_or("none").to_string(),
                    model: sf.get("model").and_then(|v| v.as_str()).unwrap_or("dsv4").to_string(),
                    verify_ssl: sf.get("verify_ssl").and_then(|v| v.as_bool()).unwrap_or(false),
                };
                let hotkey = HotkeyConfig {
                    hold_ms: sf.get("hold_ms").and_then(|v| v.as_u64()).unwrap_or(1500),
                };
                let asr = AsrConfig {
                    model_dir: PathBuf::from(
                        sf.get("model_dir").and_then(|v| v.as_str()).unwrap_or("models/sense-voice-int8"),
                    ),
                };
                return Some(LoadedSettings { llm, hotkey, asr });
            }
        }
    }
    None
}

fn load_llm_config() -> LlmConfig {
    let candidates = [
        PathBuf::from("models.yaml"),
        PathBuf::from("assets/models.yaml"),
    ];
    for path in &candidates {
        if let Ok(content) = std::fs::read_to_string(path) {
            if let Ok(file) = serde_yaml::from_str::<ModelsFile>(&content) {
                return LlmConfig {
                    base_url: file.base_url,
                    api_key: file.api_key.unwrap_or_else(|| "none".to_string()),
                    model: file.model,
                    verify_ssl: file.verify_ssl.unwrap_or(false),
                };
            }
        }
    }

    LlmConfig {
        base_url: "http://122.1.231.24:8000/v1".to_string(),
        api_key: "none".to_string(),
        model: "dsv4".to_string(),
        verify_ssl: false,
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