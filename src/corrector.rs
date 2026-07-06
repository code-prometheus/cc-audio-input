//! LLM 修正器 — Claude Code 编程场景专用
//!
//! 将 ASR 原始文本 + 编程热词上下文发给 LLM：
//! 1. 音近词替换为正确的编程术语（参考热词表）
//! 2. 通读上下文确认语义连贯
//! 3. 与编程无关的口语/闲聊内容删除
//! 4. 输出整理后的编程相关文本

use anyhow::{Context, Result};
use log::info;
use serde::{Deserialize, Serialize};
use std::time::Duration;

use crate::config::LlmConfig;
use crate::hotwords::Hotwords;

pub struct Corrector {
    settings: LlmConfig,
    hotwords: Hotwords,
    client: reqwest::blocking::Client,
}

#[derive(Debug, Serialize)]
struct ChatRequest {
    model: String,
    messages: Vec<ChatMessage>,
    max_tokens: u32,
    temperature: f32,
}

#[derive(Debug, Serialize)]
struct ChatMessage {
    role: String,
    content: String,
}

#[derive(Debug, Deserialize)]
struct ChatResponse {
    choices: Vec<ChatChoice>,
}

#[derive(Debug, Deserialize)]
struct ChatChoice {
    message: ChatMsgContent,
}

#[derive(Debug, Deserialize)]
struct ChatMsgContent {
    content: String,
}

impl Corrector {
    pub fn new(settings: &LlmConfig, hotwords: &Hotwords) -> Result<Self> {
        let mut builder = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(15));

        if !settings.verify_ssl {
            builder = builder.danger_accept_invalid_certs(true);
        }

        let client = builder.build()
            .context("创建 HTTP 客户端失败")?;

        Ok(Self {
            settings: settings.clone(),
            hotwords: hotwords.clone(),
            client,
        })
    }

    pub fn correct(&self, raw_text: &str) -> Result<String> {
        // 本地快速音近替换
        let text = self.hotwords.quick_correct(raw_text);
        let prompt = self.build_correction_prompt(&text);
        info!("🔧 修正prompt长度: {} chars, 热词映射: {} 条",
              prompt.len(), self.hotwords.phonetic_count());

        let request = ChatRequest {
            model: self.settings.model.clone(),
            messages: vec![
                ChatMessage {
                    role: "system".to_string(),
                    content: "你是Claude Code编程语音修正器。输入是语音识别(ASR)文本,已经过初步音近词替换。你需要在编程上下文中做二次修正:\n\n1. **识别孤立大写字母/短字母**: 如果文本中出现看不懂的孤立字母(如Q/K/G/V等)或短字母组合,很可能是ASR把整词误识别为字母——尝试根据上下文和参考表将其替换为正确的编程术语(例如Q→Claude Code, G→GitHub, V→VSCode)\n2. 对照参考表检查音近词,用正确术语替换(close→Claude, 卡狗→cargo等)\n3. 修正后通读——不通顺则重新调整\n4. 删除与编程完全无关的闲聊\n5. 输出为清晰直接的自然语言\n\n输出规则: 只输出修正后的文本,不加前缀/引号/解释。与编程无关则输出(空)。".to_string(),
                },
                ChatMessage {
                    role: "user".to_string(),
                    content: prompt,
                },
            ],
            max_tokens: 256,
            temperature: 0.0,
        };

        let url = format!("{}/chat/completions", self.settings.base_url.trim_end_matches('/'));
        info!("🔧 调用 LLM: {} @ {}", self.settings.model, url);

        let mut req = self.client
            .post(&url)
            .header("Content-Type", "application/json")
            .json(&request);

        if self.settings.api_key != "none" && !self.settings.api_key.is_empty() {
            req = req.header("Authorization", format!("Bearer {}", self.settings.api_key));
        }

        let response = req.send()
            .context(format!("LLM API 请求失败: {}", url))?;

        let status = response.status();
        if !status.is_success() {
            let body = response.text().unwrap_or_default();
            return Err(anyhow::anyhow!("LLM API 返回错误 {}: {}", status, body));
        }

        let result: ChatResponse = response.json()
            .context("LLM API 响应解析失败")?;

        let corrected = result.choices
            .first()
            .map(|c| c.message.content.trim().to_string())
            .unwrap_or(text);

        Ok(corrected)
    }

    fn build_correction_prompt(&self, raw_text: &str) -> String {
        let hotwords_context = self.hotwords.get_prompt_context();
        format!(
            "音近词替换参考（ASR误识→正确术语, 请在修正时参照此表）：\n{}\n\n\
             经快速替换后的文本（可能仍有漏配/错配的音近词,需你二次修正）：\n「{}」\n\n\
             请输出最终修正后文本：",
            hotwords_context,
            raw_text
        )
    }
}
