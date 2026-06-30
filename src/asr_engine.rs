//! ASR 引擎 — 通过 sherpa-onnx CLI 子进程调用 SenseVoice
//!
//! 避免了 FFI 结构体对齐问题。
//! 将 PCM f32 音频写入临时 WAV 文件，调用 sherpa-onnx-offline.exe 识别，
//! 读取 stdout 获取结果文本。

use anyhow::{Context, Result};
use log::{info, warn, debug};
use std::io::Write;
use std::path::Path;
use std::process::Command;

pub struct AsrEngine {
    model_path: std::path::PathBuf,
    tokens_path: std::path::PathBuf,
    sherpa_exe: std::path::PathBuf,
}

impl AsrEngine {
    /// 查找 sherpa-onnx-offline.exe (在 sherpa_dll/bin/ 或同级目录)
    pub fn new(model_dir: &Path) -> Result<Self> {
        let model_path = model_dir.join("model.int8.onnx");
        let tokens_path = model_dir.join("tokens.txt");

        if !model_path.exists() {
            warn!("SenseVoice 模型不存在: {:?}", model_path);
            return Err(anyhow::anyhow!("模型文件未找到"));
        }
        if !tokens_path.exists() {
            warn!("tokens.txt 不存在: {:?}", tokens_path);
            return Err(anyhow::anyhow!("tokens.txt 未找到"));
        }

        // 查找 sherpa-onnx-offline.exe
        let sherpa_candidates = [
            Path::new("sherpa_dll/sherpa-onnx-v1.13.3-win-x64-shared-MD-Release/bin/sherpa-onnx-offline.exe"),
            Path::new("target/release/sherpa-onnx-offline.exe"),
            Path::new("sherpa_dll/sherpa-onnx-offline.exe"),
        ];
        let sherpa_exe = sherpa_candidates.iter()
            .find(|p| p.exists())
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|| Path::new("sherpa-onnx-offline.exe").to_path_buf());

        if !sherpa_exe.exists() {
            warn!("sherpa-onnx-offline.exe 未找到: {:?}", sherpa_exe);
            return Err(anyhow::anyhow!("sherpa-onnx-offline.exe 未找到"));
        }

        info!("🔧 SenseVoice CLI: {:?}", sherpa_exe);
        info!("   模型: {:?}", model_path);

        Ok(Self { model_path, tokens_path, sherpa_exe })
    }

    /// 识别 PCM f32 音频 → 文本
    pub fn recognize(&self, audio_data: &[f32], sample_rate: u32) -> Result<String> {
        // 1. 写临时 WAV 文件
        let ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let wav_path = std::env::temp_dir().join(format!("ai_{}.wav", ts));
        self.write_wav(&wav_path, audio_data, sample_rate)?;
        debug!("临时 WAV: {:?} ({} samples)", wav_path, audio_data.len());

        // 2. 调用 sherpa-onnx-offline.exe
        let model_str = self.model_path.to_str().unwrap();
        let tokens_str = self.tokens_path.to_str().unwrap();
        let wav_str = wav_path.to_str().unwrap();

        let output = Command::new(&self.sherpa_exe)
            .args([
                format!("--sense-voice-model={model_str}"),
                format!("--tokens={tokens_str}"),
                "--sense-voice-use-itn=true".to_string(),
                wav_str.to_string(),
            ])
            .output()
            .context("执行 sherpa-onnx-offline.exe 失败")?;

        // 3. 清理临时文件
        let _ = std::fs::remove_file(&wav_path);

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(anyhow::anyhow!("ASR 识别失败: {}", stderr));
        }

        // sherpa-onnx-offline 输出到 stdout
        let text = String::from_utf8_lossy(&output.stdout)
            .lines()
            .next()
            .unwrap_or("")
            .trim()
            .to_string();

        Ok(text)
    }

    /// 将 PCM f32 写入 WAV 文件
    fn write_wav(&self, path: &Path, data: &[f32], sample_rate: u32) -> Result<()> {
        let file = std::fs::File::create(path)?;
        let mut writer = std::io::BufWriter::new(file);

        let data_len = data.len() as u32;
        let byte_rate = sample_rate * 4; // 32-bit float = 4 bytes/sample

        // WAV 头 (44 bytes)
        writer.write_all(b"RIFF")?;
        writer.write_all(&(36 + data_len * 4).to_le_bytes())?;
        writer.write_all(b"WAVE")?;
        writer.write_all(b"fmt ")?;
        writer.write_all(&16u32.to_le_bytes())?;    // chunk size
        writer.write_all(&3u16.to_le_bytes())?;     // format = IEEE float
        writer.write_all(&1u16.to_le_bytes())?;     // mono
        writer.write_all(&sample_rate.to_le_bytes())?;
        writer.write_all(&byte_rate.to_le_bytes())?;
        writer.write_all(&4u16.to_le_bytes())?;     // block align
        writer.write_all(&32u16.to_le_bytes())?;    // bits per sample
        writer.write_all(b"data")?;
        writer.write_all(&(data_len * 4).to_le_bytes())?;

        // 写入采样数据
        for sample in data {
            writer.write_all(&sample.to_le_bytes())?;
        }
        writer.flush()?;

        debug!("WAV 写入: {:?}, {} samples", path, data_len);
        Ok(())
    }
}
