//! Claude Code 编程专用热词管理

use anyhow::{Context, Result};
use log::info;
use serde::Deserialize;
use std::collections::HashMap;
use std::path::Path;

#[derive(Debug, Clone, Deserialize)]
struct HotwordsFile {
    claude_code_commands: Option<Vec<String>>,
    claude_models: Option<Vec<String>>,
    dev_tools: Option<Vec<String>>,
    project_terms: Option<Vec<String>>,
    programming_concepts: Option<Vec<String>>,
    phonetic_corrections: Option<HashMap<String, String>>,
    filler_words: Option<Vec<String>>,
}

#[derive(Debug, Clone)]
pub struct Hotwords {
    words: Vec<String>,
    phonetic_map: HashMap<String, String>,
    filler_words: Vec<String>,
}

impl Hotwords {
    pub fn load(path: &Path) -> Result<Self> {
        let candidates = [
            path.to_path_buf(),
            Path::new("assets/hotwords.yaml").to_path_buf(),
            Path::new("hotwords.yaml").to_path_buf(),
        ];
        let found = candidates.iter().find(|p| p.exists())
            .ok_or_else(|| anyhow::anyhow!("找不到热词文件"))?;
        let content = std::fs::read_to_string(found)
            .with_context(|| format!("无法读取热词文件: {:?}", found))?;
        let hf: HotwordsFile = serde_yaml::from_str(&content)
            .with_context(|| format!("热词YAML解析失败: {:?}", path))?;

        let mut words = Vec::new();
        for cat in [&hf.claude_code_commands, &hf.claude_models, &hf.dev_tools,
                    &hf.project_terms, &hf.programming_concepts] {
            if let Some(list) = cat {
                words.extend(list.iter().cloned());
            }
        }
        words.sort();
        words.dedup();

        info!("📖 热词加载: {} 词, {} 音近映射", words.len(),
              hf.phonetic_corrections.as_ref().map_or(0, |m| m.len()));

        Ok(Self {
            words,
            phonetic_map: hf.phonetic_corrections.unwrap_or_default(),
            filler_words: hf.filler_words.unwrap_or_default(),
        })
    }

    pub fn word_count(&self) -> usize { self.words.len() }
    pub fn phonetic_count(&self) -> usize { self.phonetic_map.len() }

    /// 本地快速音近词替换
    pub fn quick_correct(&self, text: &str) -> String {
        let mut result = text.to_string();
        let mut pairs: Vec<(&String, &String)> = self.phonetic_map.iter().collect();
        pairs.sort_by(|a, b| b.0.len().cmp(&a.0.len())); // 长匹配优先

        for (wrong, correct) in &pairs {
            let lower_text = result.to_lowercase();
            let lower_wrong = wrong.to_lowercase();
            if let Some(pos) = lower_text.find(&lower_wrong) {
                let end = pos + wrong.len();
                let replacement = if result[pos..end].chars().next().map_or(false, |c| c.is_uppercase()) {
                    let mut ch = correct.chars();
                    if let Some(first) = ch.next() {
                        format!("{}{}", first.to_uppercase(), ch.collect::<String>())
                    } else {
                        correct.to_string()
                    }
                } else {
                    correct.to_string()
                };
                result.replace_range(pos..end, &replacement);
            }
        }
        result
    }

    /// 构建 LLM prompt 上下文（精简版，给 LLM 做二次修正用）
    pub fn get_prompt_context(&self) -> String {
        let mut ctx = String::new();

        // Claude Code 命令（最优先）
        let cc: Vec<&str> = self.words.iter()
            .filter(|w| w.starts_with('/'))
            .take(25).map(|s| s.as_str()).collect();
        if !cc.is_empty() {
            ctx.push_str(&format!("Claude Code 命令: {}\n", cc.join(", ")));
        }

        // 开发工具
        let tools: Vec<&str> = self.words.iter()
            .filter(|w| !w.starts_with('/') && !w.starts_with('-') && !w.starts_with("audio-")
                    && !w.starts_with("sherpa") && !w.starts_with("Sense") && !w.starts_with("ONNX")
                    && !w.starts_with("WASAPI") && !w.starts_with("ASR") && !w.starts_with("LLM")
                    && !w.starts_with("VAD") && !w.starts_with("hotwords") && !w.starts_with("corrector"))
            .map(|s| s.as_str()).collect();
        if !tools.is_empty() {
            let joined: Vec<&str> = tools.iter().take(60).copied().collect();
            ctx.push_str(&format!("编程工具/概念: {}\n", joined.join(", ")));
        }

        // 音近映射（给LLM参考）
        if !self.phonetic_map.is_empty() {
            let pairs: Vec<String> = self.phonetic_map.iter().take(30)
                .map(|(k, v)| format!("{}→{}", k, v)).collect();
            ctx.push_str(&format!("常见音近替换: {}\n", pairs.join("; ")));
        }

        ctx
    }
}
