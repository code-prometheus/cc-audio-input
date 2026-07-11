//! 配置模块 — settings.json + models.yaml 双源加载

use std::path::PathBuf;
use serde::Deserialize;

#[derive(Debug, Clone)]
pub struct AppConfig {
    pub llm: LlmConfig,
    pub llm_models: Vec<LlmModelEntry>,
    pub active_llm_idx: usize,
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

#[derive(Debug, Clone, Deserialize)]
pub struct LlmModelEntry {
    pub name: String,
    pub base_url: String,
    pub api_key: String,
    pub model: String,
    pub verify_ssl: Option<bool>,
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

// ── settings.json 的扁平结构 ──
#[derive(Debug, Deserialize)]
struct SettingsFile {
    llm: Option<SettingsLlm>,
    hotkey: Option<SettingsHotkey>,
    asr: Option<SettingsAsr>,
}

#[derive(Debug, Deserialize)]
struct SettingsLlm {
    base_url: Option<String>,
    api_key: Option<String>,
    model: Option<String>,
    verify_ssl: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct SettingsHotkey {
    hold_ms: Option<u64>,
}

#[derive(Debug, Deserialize)]
struct SettingsAsr {
    model_dir: Option<String>,
}

// ── models.yaml 结构 ──
#[derive(Debug, Deserialize)]
struct LlmModelsFile {
    models: Vec<LlmModelEntry>,
    active: Option<String>,
}

impl AppConfig {
    pub fn load() -> Self {
        // 1. 先尝试 settings.json
        if let Some(s) = load_settings_json() {
            return Self {
                llm: s.llm,
                llm_models: s.models,
                active_llm_idx: s.active_idx,
                hotkey: s.hotkey,
                audio: AudioConfig {
                    device_id: env_i32("DEVICE_ID", -1),
                    sample_rate: 16000,
                    channels: 1,
                },
                asr: s.asr,
            };
        }

        // 2. 尝试 models.yaml
        let (llm, llm_models, active_idx) = load_llm_models();

        Self {
            llm,
            llm_models,
            active_llm_idx: active_idx,
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
    models: Vec<LlmModelEntry>,
    active_idx: usize,
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
            if let Ok(sf) = serde_json::from_str::<SettingsFile>(&content) {
                let llm_cfg = LlmConfig {
                    base_url: sf.llm.as_ref().and_then(|l| l.base_url.clone())
                        .unwrap_or_else(|| "http://122.1.231.24:8000/v1".to_string()),
                    api_key: sf.llm.as_ref().and_then(|l| l.api_key.clone())
                        .unwrap_or_else(|| "none".to_string()),
                    model: sf.llm.as_ref().and_then(|l| l.model.clone())
                        .unwrap_or_else(|| "dsv4".to_string()),
                    verify_ssl: sf.llm.as_ref().and_then(|l| l.verify_ssl).unwrap_or(false),
                };
                let entry = LlmModelEntry {
                    name: llm_cfg.model.clone(),
                    base_url: llm_cfg.base_url.clone(),
                    api_key: llm_cfg.api_key.clone(),
                    model: llm_cfg.model.clone(),
                    verify_ssl: Some(llm_cfg.verify_ssl),
                };
                let hotkey = HotkeyConfig {
                    hold_ms: sf.hotkey.as_ref().and_then(|h| h.hold_ms).unwrap_or(1500),
                };
                let asr = AsrConfig {
                    model_dir: PathBuf::from(
                        sf.asr.as_ref()
                            .and_then(|a| a.model_dir.clone())
                            .unwrap_or_else(|| "models/sense-voice-int8".to_string()),
                    ),
                };
                return Some(LoadedSettings {
                    llm: llm_cfg,
                    models: vec![entry],
                    active_idx: 0,
                    hotkey,
                    asr,
                });
            }
        }
    }
    None
}

fn load_llm_models() -> (LlmConfig, Vec<LlmModelEntry>, usize) {
    let candidates = [
        PathBuf::from("models.yaml"),
        PathBuf::from("assets/models.yaml"),
    ];
    for path in &candidates {
        if let Ok(content) = std::fs::read_to_string(path) {
            if let Ok(file) = serde_yaml::from_str::<LlmModelsFile>(&content) {
                if file.models.is_empty() {
                    continue;
                }
                let active_idx = file
                    .active
                    .as_ref()
                    .and_then(|a| file.models.iter().position(|m| &m.name == a))
                    .unwrap_or(0);
                let entry = &file.models[active_idx];
                return (
                    LlmConfig {
                        base_url: entry.base_url.clone(),
                        api_key: entry.api_key.clone(),
                        model: entry.model.clone(),
                        verify_ssl: entry.verify_ssl.unwrap_or(false),
                    },
                    file.models,
                    active_idx,
                );
            }
        }
    }

    let default_llm = LlmConfig {
        base_url: "http://122.1.231.24:8000/v1".to_string(),
        api_key: "none".to_string(),
        model: "dsv4".to_string(),
        verify_ssl: false,
    };
    let default_entry = LlmModelEntry {
        name: "默认 (dsv4)".to_string(),
        base_url: default_llm.base_url.clone(),
        api_key: default_llm.api_key.clone(),
        model: default_llm.model.clone(),
        verify_ssl: Some(false),
    };
    (default_llm, vec![default_entry], 0)
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
