//! audio-input v0.4
//! 按住鼠标左键1.5秒 → 录音 → SenseVoice ASR → LLM修正 → Ctrl+V
//! 托盘菜单: 手动录音/切换麦克风/切换LLM/拷贝结果/退出

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

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
use std::sync::{Arc, Mutex, mpsc};
use std::sync::atomic::{AtomicBool, Ordering};

fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
        .format_timestamp_millis()
        .init();

    info!("🚀 audio-input v0.4");

    let cfg = config::AppConfig::load();
    info!("✅ LLM: {} @ {}", cfg.llm.model, cfg.llm.base_url);

    let hw = hotwords::Hotwords::load(&std::path::PathBuf::from("hotwords.yaml"))
        .expect("Failed to load hotwords.yaml");

    let input_id = device_selector::resolve_input_device();
    info!("🎤 输入设备: {}", device_selector::input_device_name(input_id));

    let asr = asr_engine::AsrEngine::new(&cfg.asr.model_dir)
        .map(Some)
        .unwrap_or_else(|e| {
            warn!("ASR 不可用: {} — 占位", e);
            None
        });
    let asr = Arc::new(asr);

    let corrector = corrector::Corrector::new(&cfg.llm, &hw).expect("Failed LLM corrector");
    let corrector = Arc::new(Mutex::new(corrector));
    info!("✅ LLM corrector ready");

    // ── 手动触发通道 ──
    let (trigger_tx, trigger_rx) = mpsc::channel::<()>();

    // 收集设备列表和 LLM 模型列表用于托盘子菜单
    let input_devices: Vec<String> = device_selector::list_input_devices()
        .iter()
        .map(|d| format!("{} ({}ch {}Hz)", d.name, d.channels, d.sample_rate))
        .collect();
    let llm_model_names: Vec<String> = cfg.llm_models.iter().map(|m| m.name.clone()).collect();

    let (tray_mgr, last_result) = tray::TrayManager::create(
        "audio-input 🎤",
        trigger_tx,
        input_devices,
        0,
        llm_model_names,
        cfg.active_llm_idx,
    )
    .unwrap_or_else(|e| {
        warn!("托盘失败: {}", e);
        (
            tray::TrayManager::stub(),
            Arc::new(Mutex::new(String::new())),
        )
    });
    let tray_mgr = Arc::new(tray_mgr);

    let paster = clipboard_paste::ClipboardPaster::new();
    let is_recording = Arc::new(AtomicBool::new(false));
    let audio_buffer: Arc<Mutex<Vec<f32>>> = Arc::new(Mutex::new(Vec::new()));
    let hold_ms = cfg.hotkey.hold_ms;
    let sample_rate = 16000u32;
    let channels = 1u16;

    info!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    info!("🎤 Ready! Hold left mouse {}s to record", hold_ms / 1000);
    info!("  或右键托盘 → 开始录音");
    info!("  输入设备: {}", device_selector::input_device_name(input_id));
    info!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");

    trigger::listen(
        hold_ms,
        trigger_rx,
        // on_trigger: 开始录音
        {
            let is_rec = is_recording.clone();
            let audio_buf = audio_buffer.clone();
            let tray = tray_mgr.clone();
            move || {
                tray.show_notification("audio-input", "🔴 录音中...");
                is_rec.store(true, Ordering::SeqCst);
                info!("🔴 Recording...");
                let is_rec = is_rec.clone();
                let audio_buf = audio_buf.clone();
                std::thread::spawn(move || {
                    // ★ 三次尖锐短蜂鸣: "哔-哔-哔" — 清晰提示开始讲话
                    #[cfg(windows)]
                    unsafe {
                        let beep = windows::Win32::System::Diagnostics::Debug::Beep;
                        beep(2000, 100).ok();
                        std::thread::sleep(std::time::Duration::from_millis(80));
                        beep(2000, 100).ok();
                        std::thread::sleep(std::time::Duration::from_millis(80));
                        beep(2400, 150).ok();
                    }
                    let rec_cfg = recorder::RecorderConfig {
                        sample_rate,
                        device_id: input_id,
                        channels,
                    };
                    if let Err(e) =
                        recorder::record_blocking(&rec_cfg, is_rec, &audio_buf)
                    {
                        error!("Record error: {}", e);
                    }
                });
            }
        },
        // on_release: 停止录音 → ASR → LLM → 粘贴
        {
            let is_rec = is_recording.clone();
            let audio_buf = audio_buffer.clone();
            let tray = tray_mgr.clone();
            let last_res = last_result.clone();
            let corrector = corrector.clone();
            let asr = asr.clone();
            move || {
                is_rec.store(false, Ordering::SeqCst);
                std::thread::sleep(std::time::Duration::from_millis(200));

                let audio_data = audio_buf.lock().unwrap().clone();
                if audio_data.is_empty() {
                    info!("⚠️ No audio");
                    return;
                }
                let dur = audio_data.len() as f64 / sample_rate as f64;
                info!("📊 Audio: {:.1}s", dur);

                let raw = match asr.as_ref().as_ref() {
                    Some(e) => e.recognize(&audio_data, sample_rate).unwrap_or_else(|e| {
                        error!("ASR: {}", e);
                        "[ASR Error]".into()
                    }),
                    None => format!("[占位-{:.1}s]", dur),
                };
                info!("📝 ASR: {}", raw);

                let final_text = match corrector.lock().unwrap().correct(&raw) {
                    Ok(t) => {
                        info!("🔧 Corrected: {}", t);
                        t
                    }
                    Err(e) => {
                        warn!("LLM failed: {}", e);
                        raw
                    }
                };

                tray.update_result(&final_text);
                if let Ok(mut lr) = last_res.lock() {
                    *lr = final_text.clone();
                }

                match paster.paste(&final_text) {
                    Ok(()) => info!("📋✅ Pasted: {}", final_text),
                    Err(e) => error!("Paste: {}", e),
                }
                audio_buf.lock().unwrap().clear();
                tray.show_notification("audio-input", "✅ 已粘贴到CLI");
                info!("✅ Ready");
            }
        },
    );
}
