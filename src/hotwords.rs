//! Claude Code 编程语音 — 音近词修正热词管理
//!
//! 从 hotwords.yaml 加载分类的音近词映射表(foo→bar),
//! 提供本地快速替换 + LLM 上下文构建。

use anyhow::{Context, Result};
use log::info;
use serde::Deserialize;
use std::collections::HashMap;
use std::path::Path;

#[derive(Debug, Clone, Deserialize)]
struct HotwordsFile {
    claude_ai: Option<HashMap<String, String>>,
    dev_platform: Option<HashMap<String, String>>,
    version_control: Option<HashMap<String, String>>,
    cicd: Option<HashMap<String, String>>,
    packaging: Option<HashMap<String, String>>,
    rust: Option<HashMap<String, String>>,
    programming: Option<HashMap<String, String>>,
    project: Option<HashMap<String, String>>,
    filesystem: Option<HashMap<String, String>>,
    filler_words: Option<Vec<String>>,
}

#[derive(Debug, Clone)]
pub struct Hotwords {
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

        // 合并所有分类的音近映射
        let mut phonetic_map = HashMap::new();
        for cat in [hf.claude_ai, hf.dev_platform, hf.version_control,
                    hf.cicd, hf.packaging, hf.rust, hf.programming,
                    hf.project, hf.filesystem] {
            if let Some(map) = cat {
                phonetic_map.extend(map);
            }
        }

        info!("📖 热词加载: {} 音近映射, {} 填充词",
              phonetic_map.len(),
              hf.filler_words.as_ref().map_or(0, |v| v.len()));

        Ok(Self {
            phonetic_map,
            filler_words: hf.filler_words.unwrap_or_default(),
        })
    }

    pub fn phonetic_count(&self) -> usize { self.phonetic_map.len() }

    /// 本地快速音近词替换: 长匹配优先, 多次迭代直到稳定
    /// - 短词(≤2字符) 仅在词边界处替换 (避免 "Q" 误匹配 "Query")
    /// - 长词(>2字符) 继续用子串匹配
    /// - 重复遍历直到没有任何替换发生 (处理嵌套替换: "Q code"→"Claude Code")
    pub fn quick_correct(&self, text: &str) -> String {
        let mut result = text.to_string();
        let mut pairs: Vec<(&String, &String)> = self.phonetic_map.iter().collect();
        pairs.sort_by(|a, b| b.0.len().cmp(&a.0.len())); // 长匹配优先

        // 迭代替换, 最多10轮 (避免死循环)
        for _round in 0..10 {
            let before = result.clone();
            for (wrong, correct) in &pairs {
                if wrong.len() <= 2 {
                    result = replace_word_boundary(&result, wrong, correct);
                } else {
                    let lower_text = result.to_lowercase();
                    let lower_wrong = wrong.to_lowercase();
                    if let Some(pos) = lower_text.find(&lower_wrong) {
                        let end = pos + wrong.len();
                        let replacement = capitalize_match(&result[pos..end], correct);
                        result.replace_range(pos..end, &replacement);
                    }
                }
            }
            if result == before { break; } // 无变化, 退出
        }
        result
    }

    /// 构建 LLM prompt 上下文, 输出全部音近映射 (按误识别词字母序稳定排列)
    pub fn get_prompt_context(&self) -> String {
        let mut ctx = String::new();
        ctx.push_str("编程音近词替换参考（ASR误识→正确术语，全部映射）：\n");

        // 稳定排序: 按误识别词(key)字母序, 输出全部映射
        let mut pairs: Vec<(&String, &String)> = self.phonetic_map.iter().collect();
        pairs.sort_by(|a, b| a.0.cmp(b.0));
        let formatted: Vec<String> = pairs.iter()
            .map(|(k, v)| format!("{}→{}", k, v))
            .collect();
        ctx.push_str(&formatted.join("；"));
        ctx.push('\n');

        if !self.filler_words.is_empty() {
            ctx.push_str(&format!("口语填充词(需移除): {}\n", self.filler_words.join("、")));
        }

        ctx
    }
}

/// 词边界替换: 只在 wrong 是独立词时替换 (前后为空格/标点/字符串边界)
fn replace_word_boundary(text: &str, wrong: &str, correct: &str) -> String {
    let lower = text.to_lowercase();
    let lower_wrong = wrong.to_lowercase();
    let mut result = text.to_string();
    let mut offset = 0isize;

    for (pos, _) in lower.match_indices(&lower_wrong) {
        let adj_pos = (pos as isize + offset) as usize;
        if adj_pos >= result.len() { break; }

        let prev_char = if adj_pos == 0 { None } else { result[..adj_pos].chars().last() };
        let end = adj_pos + wrong.len();
        let next_char = result[end..].chars().next();

        let is_start = adj_pos == 0 || is_boundary_char(prev_char);
        let is_end = end >= result.len() || is_boundary_char(next_char);

        if is_start && is_end {
            let replacement = capitalize_match(&result[adj_pos..end], correct);
            let old_len = result.len();
            result.replace_range(adj_pos..end, &replacement);
            offset += result.len() as isize - old_len as isize;
        }
    }
    result
}

/// 边界字符: 空格/标点 = 词边界
fn is_boundary_char(c: Option<char>) -> bool {
    match c {
        Some(ch) => matches!(ch, ' ' | '\t' | '\n' | '.' | ',' | ';' | ':' | '?' | '!'
                             | '(' | ')' | '[' | ']' | '{' | '}' | '"' | '\'' | '/' | '\\' | '-'
                             | '。' | '，' | '；' | '：' | '？' | '！'),
        None => true,
    }
}

/// 保持原文大小写风格
fn capitalize_match(original: &str, correct: &str) -> String {
    if original.chars().next().map_or(false, |c| c.is_uppercase()) {
        let mut ch = correct.chars();
        if let Some(first) = ch.next() {
            format!("{}{}", first.to_uppercase(), ch.collect::<String>())
        } else {
            correct.to_string()
        }
    } else {
        correct.to_string()
    }
}
