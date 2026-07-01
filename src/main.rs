//! audio-input v0.3
//! 按住鼠标左键3秒 → "嘀" → 录音 → SenseVoice ASR → LLM修正 → Ctrl+V

mod config;
mod trigger;
mod recorder;
mod asr_engine;
mod hotwords;
mod corrector;
mod clipboard_paste;
mod device_selector;
mod tray;

use log::{info, error, warn};
use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicBool, Ordering};

fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
        .format_timestamp_millis()
        .init();

    info!("🚀 audio-input v0.3");

    let cfg = config::AppConfig::load();
    info!("✅ LLM: {} @ {}", cfg.llm.model, cfg.llm.base_url);

    let hw = hotwords::Hotwords::load(
        &std::path::PathBuf::from("hotwords.yaml")
    ).expect("Failed to load hotwords.yaml");

    // ★ 设备选择
    let input_id = device_selector::resolve_input_device();
    let output_id = device_selector::resolve_output_device();
    info!("🎤 输入: {} | 🎧 输出: {}",
        device_selector::input_device_name(input_id),
        device_selector::output_device_name(output_id));

    let asr = asr_engine::AsrEngine::new(&cfg.asr.model_dir)
        .map(Some).unwrap_or_else(|e| { warn!("ASR 不可用: {} — 占位", e); None });

    let corrector = corrector::Corrector::new(&cfg.llm, &hw)
        .expect("Failed LLM corrector");
    info!("✅ LLM corrector ready");

    let (tray_mgr, last_result) = tray::TrayManager::create("audio-input 🎤")
        .unwrap_or_else(|e| { warn!("托盘失败: {}", e); (tray::TrayManager::stub(), Arc::new(Mutex::new(String::new()))) });

    let paster = clipboard_paste::ClipboardPaster::new();
    let is_recording = Arc::new(AtomicBool::new(false));
    let audio_buffer: Arc<Mutex<Vec<f32>>> = Arc::new(Mutex::new(Vec::new()));
    let hold_ms = cfg.hotkey.hold_ms;
    let sample_rate = 16000u32;
    let channels = 1u16;

    info!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    info!("🎤 Ready! Hold left mouse {}s to record", hold_ms / 1000);
    info!("   输入:{} 输出:{}",
        device_selector::input_device_name(input_id),
        device_selector::output_device_name(output_id));
    info!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");

    trigger::listen(
        hold_ms,
        // on_trigger: 按住3秒 → 释放检测 → 播放提示音 + 开始录音
        {
            let is_rec = is_recording.clone();
            let audio_buf = audio_buffer.clone();
            move || {
                // ★ 鼠标检测已释放，启动录音线程
                is_rec.store(true, Ordering::SeqCst);
                info!("🔴 Recording...");
                let is_rec = is_rec.clone();
                let audio_buf = audio_buf.clone();
                std::thread::spawn(move || {
                    // ★ 先同步播放提示音（等待播完），再开麦克风
                    //    麦克风的 WASAPI 输入流会终止异步播放
                    play_beep_sync(output_id, 1800, 80);
                    let rec_cfg = recorder::RecorderConfig { sample_rate, device_id: input_id, channels };
                    if let Err(e) = recorder::record_blocking(&rec_cfg, is_rec, &audio_buf) {
                        error!("Record error: {}", e);
                    }
                });
            }
        },
        // on_release
        {
            let is_rec = is_recording.clone();
            let audio_buf = audio_buffer.clone();
            let tray = tray_mgr;
            let last_res = last_result.clone();
            move || {
                is_rec.store(false, Ordering::SeqCst);
                std::thread::sleep(std::time::Duration::from_millis(200));
                // ★ 低沉短"咚" — 600Hz 80ms
                play_beep(output_id, 600, 80);

                let audio_data = audio_buf.lock().unwrap().clone();
                if audio_data.is_empty() { info!("⚠️  No audio"); return; }
                let dur = audio_data.len() as f64 / sample_rate as f64;
                info!("📊 Audio: {:.1}s", dur);

                let raw = match &asr {
                    Some(e) => e.recognize(&audio_data, sample_rate).unwrap_or_else(|e| { error!("ASR: {}", e); "[ASR Error]".into() }),
                    None => format!("[占位-{:.1}s]", dur),
                };
                info!("📝 ASR: {}", raw);

                let final_text = match corrector.correct(&raw) {
                    Ok(t) => { info!("🔧 Corrected: {}", t); t }
                    Err(e) => { warn!("LLM failed: {}", e); raw }
                };

                tray.update_result(&final_text);
                if let Ok(mut lr) = last_res.lock() { *lr = final_text.clone(); }
                tray.show_notification("audio-input", &final_text);

                match paster.paste(&final_text) {
                    Ok(()) => info!("📋✅ Pasted: {}", final_text),
                    Err(e) => error!("Paste: {}", e),
                }
                audio_buf.lock().unwrap().clear();
                info!("✅ Ready");
            }
        },
    );
}

/// 生成短促WAV临时文件并播放 (PlaySoundW SND_FILENAME 不会被吞)
fn play_beep(_output_device_id: i32, freq: u32, duration_ms: u64) {
    let sample_rate = 44100u32;
    let tone_samples = (sample_rate as u64 * duration_ms / 1000) as usize;
    let mut samples = vec![0u8; 44 + tone_samples * 2]; // 16-bit PCM

    // 生成 16-bit PCM 波形
    let amp = 12000i16;
    for i in 0..tone_samples {
        let t = i as f32 / sample_rate as f32;
        let val = (t * freq as f32 * 2.0 * std::f32::consts::PI).sin();
        let env = if i < tone_samples / 5 { i as f32 / (tone_samples / 5) as f32 }
                  else if i > tone_samples * 4 / 5 { (tone_samples - i) as f32 / (tone_samples / 5) as f32 }
                  else { 1.0 };
        let sample = (val * env * amp as f32) as i16;
        let offset = 44 + i * 2;
        samples[offset] = (sample & 0xFF) as u8;
        samples[offset + 1] = ((sample >> 8) & 0xFF) as u8;
    }

    // WAV 头
    let total_size = 36 + tone_samples as u32 * 2;
    samples[0..4].copy_from_slice(b"RIFF");
    samples[4..8].copy_from_slice(&total_size.to_le_bytes());
    samples[8..12].copy_from_slice(b"WAVE");
    samples[12..16].copy_from_slice(b"fmt ");
    samples[16..20].copy_from_slice(&16u32.to_le_bytes());  // PCM
    samples[20..22].copy_from_slice(&1u16.to_le_bytes());   // PCM=1
    samples[22..24].copy_from_slice(&1u16.to_le_bytes());   // mono
    samples[24..28].copy_from_slice(&sample_rate.to_le_bytes());
    samples[28..32].copy_from_slice(&(sample_rate * 2).to_le_bytes()); // byte rate (16-bit = 2 bytes)
    samples[32..34].copy_from_slice(&2u16.to_le_bytes());   // block align
    samples[34..36].copy_from_slice(&16u16.to_le_bytes());  // bits per sample
    samples[36..40].copy_from_slice(b"data");
    samples[40..44].copy_from_slice(&(tone_samples as u32 * 2).to_le_bytes());

    // 写临时文件
    let tmp = std::env::temp_dir().join(format!("beep_{}.wav", std::process::id()));
    if let Ok(()) = std::fs::write(&tmp, &samples) {
        let path: Vec<u16> = tmp.to_str().unwrap_or("").encode_utf16().chain(std::iter::once(0)).collect();
        #[cfg(windows)]
        unsafe {
            use windows::Win32::Media::Audio::*;
            use windows::core::PCWSTR;
            // SND_FILENAME | SND_ASYNC = 播放文件，异步不阻塞，一定出声
            PlaySoundW(PCWSTR::from_raw(path.as_ptr()), None, SND_FILENAME | SND_ASYNC);
        }
        // 延迟删除临时文件
        std::thread::sleep(std::time::Duration::from_millis(duration_ms + 100));
        let _ = std::fs::remove_file(&tmp);
    }
}

/// 同步版 — 播放完毕才返回，保证声音不被后续麦克风打开终止
fn play_beep_sync(_output_device_id: i32, freq: u32, duration_ms: u64) {
    let sample_rate = 44100u32;
    let tone_samples = (sample_rate as u64 * duration_ms / 1000) as usize;
    let mut samples = vec![0u8; 44 + tone_samples * 2];

    let amp = 12000i16;
    for i in 0..tone_samples {
        let t = i as f32 / sample_rate as f32;
        let val = (t * freq as f32 * 2.0 * std::f32::consts::PI).sin();
        let env = if i < tone_samples / 5 { i as f32 / (tone_samples / 5) as f32 }
                  else if i > tone_samples * 4 / 5 { (tone_samples - i) as f32 / (tone_samples / 5) as f32 }
                  else { 1.0 };
        let sample = (val * env * amp as f32) as i16;
        let offset = 44 + i * 2;
        samples[offset] = (sample & 0xFF) as u8;
        samples[offset + 1] = ((sample >> 8) & 0xFF) as u8;
    }

    let total_size = 36 + tone_samples as u32 * 2;
    samples[0..4].copy_from_slice(b"RIFF");
    samples[4..8].copy_from_slice(&total_size.to_le_bytes());
    samples[8..12].copy_from_slice(b"WAVE");
    samples[12..16].copy_from_slice(b"fmt ");
    samples[16..20].copy_from_slice(&16u32.to_le_bytes());
    samples[20..22].copy_from_slice(&1u16.to_le_bytes());
    samples[22..24].copy_from_slice(&1u16.to_le_bytes());
    samples[24..28].copy_from_slice(&sample_rate.to_le_bytes());
    samples[28..32].copy_from_slice(&(sample_rate * 2).to_le_bytes());
    samples[32..34].copy_from_slice(&2u16.to_le_bytes());
    samples[34..36].copy_from_slice(&16u16.to_le_bytes());
    samples[36..40].copy_from_slice(b"data");
    samples[40..44].copy_from_slice(&(tone_samples as u32 * 2).to_le_bytes());

    let tmp = std::env::temp_dir().join(format!("beep_{}.wav", std::process::id()));
    if let Ok(()) = std::fs::write(&tmp, &samples) {
        let path: Vec<u16> = tmp.to_str().unwrap_or("").encode_utf16().chain(std::iter::once(0)).collect();
        #[cfg(windows)]
        unsafe {
            use windows::Win32::Media::Audio::*;
            use windows::core::PCWSTR;
            // ★ SND_FILENAME | SND_SYNC = 同步播放，播完才返回
            //    这样麦克风打开时声音已经播完，不会被终止
            PlaySoundW(PCWSTR::from_raw(path.as_ptr()), None, SND_FILENAME | SND_SYNC);
        }
        let _ = std::fs::remove_file(&tmp);
    }
}
