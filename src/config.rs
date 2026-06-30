//! 配置模块 — LLM 参数硬编码，热键可通过环境变量覆盖

#[derive(Debug, Clone)]
pub struct AppConfig {
    pub llm: LlmConfig,
    pub hotkey: HotkeyConfig,
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
                hold_ms: std::env::var("AUDIO_INPUT_HOLD_MS")
                    .ok()
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(3000),
            },
        }
    }
}
