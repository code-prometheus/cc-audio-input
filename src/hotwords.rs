//! CLI 热词管理

use anyhow::{Context, Result};
use serde::Deserialize;
use std::collections::HashMap;
use std::path::Path;

#[derive(Debug, Clone, Deserialize)]
struct HotwordsFile {
    claude_code_commands: Option<Vec<String>>,
    cli_tools: Option<Vec<String>>,
    common_options: Option<Vec<String>>,
    project_specific: Option<Vec<String>>,
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
        // 尝试多个路径
        let candidates = [
            path.to_path_buf(),
            Path::new("assets/hotwords.yaml").to_path_buf(),
            Path::new("hotwords.yaml").to_path_buf(),
        ];
        let found = candidates.iter().find(|p| p.exists())
            .ok_or_else(|| anyhow::anyhow!("找不到热词文件，搜索路径: {:?}", candidates))?;
        let content = std::fs::read_to_string(found)
            .with_context(|| format!("无法读取热词文件: {:?}", found))?;
        let hf: HotwordsFile = serde_yaml::from_str(&content)
            .with_context(|| format!("热词YAML解析失败: {:?}", path))?;

        let mut words = Vec::new();
        for cat in [&hf.claude_code_commands, &hf.cli_tools, &hf.common_options, &hf.project_specific] {
            if let Some(list) = cat {
                words.extend(list.iter().cloned());
            }
        }
        words.sort();
        words.dedup();

        Ok(Self {
            words,
            phonetic_map: hf.phonetic_corrections.unwrap_or_default(),
            filler_words: hf.filler_words.unwrap_or_default(),
        })
    }

    pub fn word_count(&self) -> usize { self.words.len() }
    pub fn phonetic_count(&self) -> usize { self.phonetic_map.len() }

    pub fn quick_correct(&self, text: &str) -> String {
        let mut result = text.to_string();
        let mut pairs: Vec<(&String, &String)> = self.phonetic_map.iter().collect();
        pairs.sort_by(|a, b| b.0.len().cmp(&a.0.len()));

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

    pub fn get_prompt_context(&self) -> String {
        let mut ctx = String::new();
        let cc: Vec<&str> = self.words.iter()
            .filter(|w| w.starts_with('/') || w.starts_with("claude") || w.starts_with("Claude") || w.starts_with("Fable") || w.starts_with("Opus") || w.starts_with("Sonnet") || w.starts_with("Haiku"))
            .take(30).map(|s| s.as_str()).collect();
        if !cc.is_empty() {
            ctx.push_str("Claude Code: ");
            ctx.push_str(&cc.join(", "));
            ctx.push('\n');
        }
        let tools: Vec<&str> = self.words.iter()
            .filter(|w| !w.starts_with('/') && !w.starts_with('-'))
            .take(50).map(|s| s.as_str()).collect();
        if !tools.is_empty() {
            ctx.push_str("CLI工具: ");
            ctx.push_str(&tools.join(", "));
            ctx.push('\n');
        }
        if !self.phonetic_map.is_empty() {
            ctx.push_str("音近修正: ");
            let pairs: Vec<String> = self.phonetic_map.iter().take(20)
                .map(|(k, v)| format!("{}→{}", k, v)).collect();
            ctx.push_str(&pairs.join("; "));
            ctx.push('\n');
        }
        ctx
    }
}
