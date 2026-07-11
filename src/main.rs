//! audio-input v0.5
//! 按住鼠标左键1.5秒不动 → 录音 → SenseVoice ASR → LLM修正 → Ctrl+V
//! 托盘菜单: tray-icon + winit (按经验文档)

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod config;
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
use std::io::Write;

fn init_logging() {
    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.to_path_buf()))
        .unwrap_or_else(|| std::path::PathBuf::from("."));
    let log_path = exe_dir.join("audio-input.log");
    let file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)
        .expect("无法创建日志文件");
    let mut builder =
        env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"));
    builder
        .format_timestamp_millis()
        .format(|buf, record| {
            writeln!(
                buf,
                "[{} {} {}] {}",
                buf.timestamp_millis(),
                record.level(),
                record.target(),
                record.args()
            )
        })
        .target(env_logger::Target::Pipe(Box::new(file)))
        .init();
}

fn main() {
    init_logging();
    info!("🚀 audio-input v0.5");

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

    let input_devices: Vec<String> = device_selector::list_input_devices()
        .iter()
        .map(|d| format!("{} ({}ch {}Hz)", d.name, d.channels, d.sample_rate))
        .collect();
    let llm_model_names: Vec<String> = cfg.llm_models.iter().map(|m| m.name.clone()).collect();

    let (trigger_tx, trigger_rx) = mpsc::channel::<()>();

    let paster = Arc::new(clipboard_paste::ClipboardPaster::new());
    let is_recording = Arc::new(AtomicBool::new(false));
    let audio_buffer: Arc<Mutex<Vec<f32>>> = Arc::new(Mutex::new(Vec::new()));
    let hold_ms = cfg.hotkey.hold_ms;
    let sample_rate: u32 = 16000;
    let channels: u16 = 1;

    let last_result: Arc<Mutex<String>> = Arc::new(Mutex::new(String::new()));

    info!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    info!("🎤 Ready! 按住左键{}秒不动触发", hold_ms / 1000);
    info!("  右键托盘菜单 → 开始录音");
    info!("  输入设备: {}", device_selector::input_device_name(input_id));
    info!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");

    // on_trigger: 开始录音（后台线程）
    let on_trigger = {
        let is_rec = is_recording.clone();
        let audio_buf = audio_buffer.clone();
        move || {
            is_rec.store(true, Ordering::SeqCst);
            info!("🔴 Recording...");
            let is_rec = is_rec.clone();
            let audio_buf = audio_buf.clone();
            std::thread::spawn(move || {
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
                if let Err(e) = recorder::record_blocking(&rec_cfg, is_rec, &audio_buf) {
                    error!("Record error: {}", e);
                }
            });
        }
    };

    // on_release: 停止 → ASR → LLM → 粘贴
    let on_release = {
        let is_rec = is_recording.clone();
        let audio_buf = audio_buffer.clone();
        let last_res = last_result.clone();
        let corrector = corrector.clone();
        let asr = asr.clone();
        let paster = paster.clone();
        move || {
            is_rec.store(false, Ordering::SeqCst);
            std::thread::sleep(std::time::Duration::from_millis(200));

            let audio_data = audio_buf.lock().unwrap().clone();
            if audio_data.is_empty() {
                info!("⚠️ No audio");
                return;
            }
            let dur = audio_data.len() as f64 / sample_rate as f64;
            let max_amp = audio_data
                .iter()
                .map(|s| s.abs())
                .fold(0.0f32, f32::max);
            let mean_amp =
                audio_data.iter().map(|s| s.abs()).sum::<f32>() / audio_data.len() as f32;
            info!(
                "📊 Audio: {:.1}s, max={:.6}, mean={:.6}",
                dur, max_amp, mean_amp
            );

            let raw = match asr.as_ref().as_ref() {
                Some(e) => e
                    .recognize(&audio_data, sample_rate)
                    .unwrap_or_else(|e| {
                        error!("ASR: {}", e);
                        format!("[ASR Error: {}]", e)
                    }),
                None => format!("[无ASR引擎-{:.1}s]", dur),
            };
            info!("📝 ASR 原始: '{}'", raw);

            if raw.trim().is_empty() || raw.starts_with('[') {
                info!("⚠️ ASR 无有效输出");
            }

            let final_text = match corrector.lock().unwrap().correct(&raw) {
                Ok(t) => {
                    if t.trim().is_empty() {
                        warn!("⚠️ LLM 返回空文本");
                        raw.clone()
                    } else {
                        info!("🔧 Corrected: '{}'", t);
                        t
                    }
                }
                Err(e) => {
                    warn!("LLM failed: {} — 使用 ASR 原文", e);
                    raw.clone()
                }
            };

            if let Ok(mut lr) = last_res.lock() {
                *lr = final_text.clone();
            }

            if !final_text.is_empty() && !final_text.starts_with('[') {
                match paster.paste(&final_text) {
                    Ok(()) => info!("📋✅ 已粘贴: {}", final_text),
                    Err(e) => error!("Paste: {}", e),
                }
            }
            audio_buf.lock().unwrap().clear();
            info!("✅ Ready");
        }
    };

    // ★ 主线程跑 winit EventLoop (阻塞)
    // 托盘在其中处理: 菜单事件 + 鼠标长按 + 手动触发
    tray::run_tray_main(
        "audio-input 🎤".to_string(),
        trigger_tx,
        trigger_rx,
        input_devices,
        0,
        llm_model_names,
        cfg.active_llm_idx,
        hold_ms,
        on_trigger,
        on_release,
    );
}
