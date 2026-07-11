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
        info!("修正prompt长度: {} chars, 热词映射: {} 条",
            prompt.len(), self.hotwords.phonetic_count());

        let request = ChatRequest {
            model: self.settings.model.clone(),
            messages: vec![
                ChatMessage {
                    role: "system".to_string(),
                    content: "你是CLI语音修正器。将ASR原始文本修正为正确的编程命令文本。\n规则:\n1. 根据参考表替换音近词(close→claude)\n2. 修正标点大小写\n3. 删除口语填充词(嗯啊那个)\n\n只输出修正后文本。闲聊输出(空)。".to_string(),
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
        info!("调用 LLM: {} @ {}", self.settings.model, url);

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
            "音近词参考表(ASR误识→正确术语):\n{}\n\n经快速替换后的文本:\n「{}」\n\n请输出修正后文本:",
            hotwords_context,
            raw_text
        )
    }
}
