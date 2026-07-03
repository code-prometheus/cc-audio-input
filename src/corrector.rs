//! LLM corrector - OpenAI compatible API
//!
//! Read API url, model, key from settings.json。
//! Send ASR text + hotword context to LLM for correction。

use anyhow::{Context, Result};
use log::info;
use serde::{Deserialize, Serialize};
use std::time::Duration;

use crate::config::LlmConfig;
use crate::hotwords::Hotwords;

/// Corrector
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
        // native-tls 方式: 通过系统证书处理 SSL，不需要 danger_accept_invalid_certs
        let mut builder = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(15));

        // 如果 verify_ssl 为 false，添加不安全的连接器
        if !settings.verify_ssl {
            builder = builder.danger_accept_invalid_certs(true);
        }

        let client = builder.build()
            .context("Failed to create HTTP client")?;

        Ok(Self {
            settings: settings.clone(),
            hotwords: hotwords.clone(),
            client,
        })
    }

    /// Execute text correction
    pub fn correct(&self, raw_text: &str) -> Result<String> {
        // Local quick correct first
        let text = self.hotwords.quick_correct(raw_text);

        let prompt = self.build_correction_prompt(&text);

        let request = ChatRequest {
            model: self.settings.model.clone(),
            messages: vec![
                ChatMessage {
                    role: "system".to_string(),
                    content: "你是一个精确的文本修正工具。只输出修正后的文本，不添加任何解释、说明或额外内容。".to_string(),
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

        // API key: "none" 表示不需要
        if self.settings.api_key != "none" && !self.settings.api_key.is_empty() {
            req = req.header("Authorization", format!("Bearer {}", self.settings.api_key));
        }

        let response = req.send()
            .context(format!("LLM API request failed: {}", url))?;

        let status = response.status();
        if !status.is_success() {
            let body = response.text().unwrap_or_default();
            return Err(anyhow::anyhow!("LLM API returned error {}: {}", status, body));
        }

        let result: ChatResponse = response.json()
            .context("LLM API response parse failed")?;

        let corrected = result.choices
            .first()
            .map(|c| c.message.content.trim().to_string())
            .unwrap_or(text);

        Ok(corrected)
    }

    fn build_correction_prompt(&self, raw_text: &str) -> String {
        let hotwords_context = self.hotwords.get_prompt_context();
        format!(
            "你是一个CLI命令Corrector。用户的语音通过ASR识别得到了原始文本。请修正以下识别文本：\n\
             1. 修正CLI tools/命令的拼写错误（参考已知术语表）\n\
             2. 标点符号和大小写规范化\n\
             3. 移除口语化填充词\n\
             4. 将音近的错误词替换为正确的CLI术语\n\n\
             已知的CLI术语/热词：\n{}\n\n\
             原始ASR文本：{}\n\n\
             请只输出修正后的文本，不要加任何解释或额外内容。",
            hotwords_context,
            raw_text
        )
    }
}
