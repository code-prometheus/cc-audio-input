//! ASR — Phase 1 占位实现

use anyhow::Result;
use log::info;

pub struct AsrEngine;

impl AsrEngine {
    pub fn new_placeholder() -> Self {
        info!("🔶 ASR 占位模式");
        Self
    }

    pub fn recognize(&self, audio_data: &[f32], sample_rate: u32) -> Result<String> {
        let duration = audio_data.len() as f64 / sample_rate as f64;
        Ok(format!("[ASR-{:.1}s]语音识别占位文本", duration))
    }
}
