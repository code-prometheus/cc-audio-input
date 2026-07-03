//! CLI 热词管理 + 本地目录名扫描
//!
//! 从 hotwords.yaml 加载术语表，提供本地快速音近纠正 + LLM 上下文构建。
//! 启动时自动扫描本地目录名，生成首字母变体映射。

use anyhow::{Context, Result};
use log::info;
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
    fs_names: Vec<String>,
}

impl Hotwords {
    pub fn load(path: &Path) -> Result<Self> {
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

        let mut phonetic_map = hf.phonetic_corrections.unwrap_or_default();

        // 扫描本地文件系统，生成目录名首字母 → 原名映射
        let fs_names = scan_local_dirs();
        info!("本地扫描到 {} 个目录名", fs_names.len());
        for name in &fs_names {
            let first_char = name.chars().next().map(|c| c.to_lowercase().to_string()).unwrap_or_default();
            if !first_char.is_empty() && first_char != name.to_lowercase() {
                phonetic_map.entry(first_char).or_insert_with(|| name.clone());
            }
        }

        Ok(Self {
            words,
            phonetic_map,
            filler_words: hf.filler_words.unwrap_or_default(),
            fs_names,
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
        if !self.fs_names.is_empty() {
            ctx.push_str("本地目录: ");
            ctx.push_str(&self.fs_names.iter().take(20).map(|s| s.as_str()).collect::<Vec<_>>().join(", "));
            ctx.push('\n');
        }
        ctx
    }
}

fn scan_local_dirs() -> Vec<String> {
    let scan_dirs: Vec<std::path::PathBuf> = [
        std::env::current_dir().ok(),
        dirs_next("DESKTOP"),
        dirs_next("DOCUMENTS"),
        dirs_next("DOWNLOAD"),
        std::env::var("USERPROFILE").ok().map(std::path::PathBuf::from),
        std::env::var("HOMEDRIVE").ok().map(|d| std::path::PathBuf::from(d + "\\")),
    ].into_iter().flatten().collect();

    let mut names = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for dir in &scan_dirs {
        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                if !entry.file_type().map(|t| t.is_dir()).unwrap_or(false) { continue; }
                let name = entry.file_name().to_string_lossy().to_string();
                if name.starts_with('.') || name.starts_with('$') || name == "System Volume Information" { continue; }
                if seen.insert(name.clone()) {
                    names.push(name);
                }
            }
        }
    }
    names
}

fn dirs_next(name: &str) -> Option<std::path::PathBuf> {
    let home = std::env::var("USERPROFILE").ok().map(std::path::PathBuf::from)?;
    match name {
        "DESKTOP" => Some(home.join("Desktop")),
        "DOCUMENTS" => Some(home.join("Documents")),
        "DOWNLOAD" => Some(home.join("Downloads")),
        _ => None,
    }
}
