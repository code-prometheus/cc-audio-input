//! ASR 引擎 — 通过 sherpa-onnx CLI 子进程调用 SenseVoice

use anyhow::{Context, Result};
use log::{info, warn};
use std::io::Write;
use std::path::Path;
use std::process::Command;
use std::os::windows::process::CommandExt;

const CREATE_NO_WINDOW: u32 = 0x08000000;

pub struct AsrEngine {
    model_path: std::path::PathBuf,
    tokens_path: std::path::PathBuf,
    sherpa_exe: std::path::PathBuf,
}

impl AsrEngine {
    pub fn new(model_dir: &Path) -> Result<Self> {
        let model_path = model_dir.join("model.int8.onnx");
        let tokens_path = model_dir.join("tokens.txt");
        if !model_path.exists() { warn!("模型: {:?}", model_path); return Err(anyhow::anyhow!("model not found")); }
        if !tokens_path.exists() { warn!("tokens: {:?}", tokens_path); return Err(anyhow::anyhow!("tokens not found")); }

        let sherpa_candidates = [
            Path::new("assets/sherpa-onnx-offline.exe"),
            Path::new("sherpa-onnx-offline.exe"),
        ];
        let sherpa_exe = sherpa_candidates.iter().find(|p| p.exists())
            .map(|p| p.to_path_buf()).unwrap_or_else(|| Path::new("sherpa-onnx-offline.exe").to_path_buf());
        if !sherpa_exe.exists() { warn!("sherpa exe: {:?}", sherpa_exe); return Err(anyhow::anyhow!("sherpa not found")); }

        info!("🔧 SenseVoice CLI: {:?}", sherpa_exe);
        info!(" 模型: {:?}", model_path);
        Ok(Self { model_path, tokens_path, sherpa_exe })
    }

    pub fn recognize(&self, audio_data: &[f32], sample_rate: u32) -> Result<String> {
        let ts = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_nanos();
        let wav_path = std::env::temp_dir().join(format!("ai_{}.wav", ts));
        self.write_wav(&wav_path, audio_data, sample_rate)
            .context("write WAV")?;

        let ms = self.model_path.to_string_lossy();
        let ts = self.tokens_path.to_string_lossy();
        let ws = wav_path.to_string_lossy();

        info!("🔮 ASR: {} samples, calling sherpa...", audio_data.len());
        let output = Command::new(&self.sherpa_exe)
            .creation_flags(CREATE_NO_WINDOW)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .args([
                format!("--sense-voice-model={}", ms),
                format!("--tokens={}", ts),
                "--sense-voice-use-itn=true".to_string(),
                ws.to_string(),
            ])
            .output()
            .context("sherpa execution failed")?;

        let _ = std::fs::remove_file(&wav_path);

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            warn!("ASR stderr: {}", stderr);
            return Err(anyhow::anyhow!("ASR failed: {}", stderr));
        }

        let raw_output = String::from_utf8_lossy(&output.stdout).to_string();
        // sherpa-onnx SenseVoice 输出 JSON 行, 提取每行的 text 字段并拼接
        let text = raw_output
            .lines()
            .filter_map(|line| {
                let line = line.trim();
                if line.is_empty() { return None; }
                serde_json::from_str::<serde_json::Value>(line).ok()
                    .and_then(|v| v.get("text").and_then(|t| t.as_str()).map(|s| s.to_string()))
            })
            .collect::<Vec<_>>()
            .join("");
        info!("🔮 ASR done: {} chars (raw {} bytes)", text.len(), raw_output.len());
        Ok(text)
    }

    fn write_wav(&self, path: &Path, data: &[f32], sample_rate: u32) -> Result<()> {
        let file = std::fs::File::create(path)?;
        let mut w = std::io::BufWriter::new(file);
        let n = data.len() as u32;
        let br = sample_rate * 4;
        w.write_all(b"RIFF")?;
        w.write_all(&(36 + n * 4).to_le_bytes())?;
        w.write_all(b"WAVE")?;
        w.write_all(b"fmt ")?;
        w.write_all(&16u32.to_le_bytes())?;
        w.write_all(&3u16.to_le_bytes())?;
        w.write_all(&1u16.to_le_bytes())?;
        w.write_all(&sample_rate.to_le_bytes())?;
        w.write_all(&br.to_le_bytes())?;
        w.write_all(&4u16.to_le_bytes())?;
        w.write_all(&32u16.to_le_bytes())?;
        w.write_all(b"data")?;
        w.write_all(&(n * 4).to_le_bytes())?;
        for s in data { w.write_all(&s.to_le_bytes())?; }
        w.flush()?;
        Ok(())
    }
}
