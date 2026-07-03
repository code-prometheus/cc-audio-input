//! Config: reads settings.json beside exe, falls back to defaults + env

use serde::Deserialize;
use std::path::PathBuf;

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct AppConfig {
    pub llm: LlmConfig,
    pub hotkey: HotkeyConfig,
    pub asr: AsrConfig,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct LlmConfig {
    pub base_url: String,
    pub api_key: String,
    pub model: String,
    pub verify_ssl: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct HotkeyConfig {
    pub hold_ms: u64,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct AsrConfig {
    pub model_dir: String,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            llm: LlmConfig::default(),
            hotkey: HotkeyConfig::default(),
            asr: AsrConfig::default(),
        }
    }
}

impl Default for LlmConfig {
    fn default() -> Self {
        Self {
            base_url: "http://localhost:8000/v1".into(),
            api_key: "none".into(),
            model: "gpt-4".into(),
            verify_ssl: false,
        }
    }
}

impl Default for HotkeyConfig {
    fn default() -> Self {
        Self { hold_ms: env_u64("HOLD_MS", 1500) }
    }
}

impl Default for AsrConfig {
    fn default() -> Self {
        Self { model_dir: String::new() }
    }
}

impl AppConfig {
    pub fn load() -> Self {
        let mut cfg = load_settings_json().unwrap_or_default();
        if let Ok(v) = std::env::var("AUDIO_INPUT_HOLD_MS") {
            if let Ok(n) = v.parse() { cfg.hotkey.hold_ms = n; }
        }
        cfg
    }

    pub fn model_dir(&self) -> PathBuf {
        // 环境变量最高优先级
        if let Ok(dir) = std::env::var("AUDIO_INPUT_MODEL_DIR") {
            let p = PathBuf::from(&dir);
            if p.join("model.int8.onnx").exists() { return p; }
        }
        // settings.json 中的配置
        if !self.asr.model_dir.is_empty() {
            let p = PathBuf::from(&self.asr.model_dir);
            if p.join("model.int8.onnx").exists() { return p; }
        }
        // exe 同级 models/sense-voice-int8/ (ZIP 解压后)
        if let Ok(exe) = std::env::current_exe() {
            if let Some(d) = exe.parent() {
                let bundled = d.join("models").join("sense-voice-int8");
                if bundled.join("model.int8.onnx").exists() { return bundled; }
            }
        }
        // 兼容旧部署
        let fallback = PathBuf::from("F:/models/sense-voice-int8");
        if fallback.join("model.int8.onnx").exists() { return fallback; }
        fallback
    }
}

fn load_settings_json() -> Option<AppConfig> {
    let candidates = [
        std::env::current_exe().ok().and_then(|p| p.parent().map(|d| d.join("settings.json"))),
        Some(PathBuf::from("settings.json")),
    ];
    for path in candidates.into_iter().flatten() {
        if path.exists() {
            if let Ok(s) = std::fs::read_to_string(&path) {
                if let Ok(c) = serde_json::from_str(&s) {
                    log::info!("Loaded settings from {}", path.display());
                    return Some(c);
                }
            }
        }
    }
    None
}

fn env_u64(key: &str, default: u64) -> u64 {
    std::env::var(format!("AUDIO_INPUT_{}", key))
        .ok().and_then(|s| s.parse().ok()).unwrap_or(default)
}
