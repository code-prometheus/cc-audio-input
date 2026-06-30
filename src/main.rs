//! audio-input v0.3
//! 按住鼠标左键3秒 → 录音 → SenseVoice ASR → LLM修正 → Ctrl+V

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

    // 热词
    let hw = hotwords::Hotwords::load(
        &std::path::PathBuf::from("hotwords.yaml")
    ).expect("Failed to load hotwords.yaml");
    info!("✅ Hotwords: {} words", hw.word_count());

    // 设备选择
    info!("🎤 可用输入设备 (设置 AUDIO_INPUT_DEVICE_ID=N 选择):");
    let _devices = device_selector::list_input_devices();
    let device_id = device_selector::resolve_device_id();
    info!("   已选择设备ID={}: {}", device_id, device_selector::device_name(device_id));

    // ASR 引擎
    let asr = asr_engine::AsrEngine::new(&cfg.asr.model_dir)
        .map(Some)
        .unwrap_or_else(|e| {
            warn!("ASR 不可用: {} — 使用占位模式", e);
            None
        });

    // LLM 修正器
    let corrector = corrector::Corrector::new(&cfg.llm, &hw)
        .expect("Failed to create LLM corrector");
    info!("✅ LLM corrector ready");

    // 系统托盘
    let (tray_mgr, last_result) = tray::TrayManager::create("audio-input 🎤")
        .unwrap_or_else(|e| {
            warn!("托盘创建失败: {} — 无托盘模式", e);
            (tray::TrayManager::stub(), Arc::new(Mutex::new(String::new())))
        });

    let paster = clipboard_paste::ClipboardPaster::new();
    let is_recording = Arc::new(AtomicBool::new(false));
    let audio_buffer: Arc<Mutex<Vec<f32>>> = Arc::new(Mutex::new(Vec::new()));
    let hold_ms = cfg.hotkey.hold_ms;
    let sample_rate = 16000u32;
    let channels = 1u16;

    info!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    info!("🎤 Ready! Hold left mouse {}s to record", hold_ms / 1000);
    info!("   模型: {:?}", cfg.asr.model_dir);
    info!("   设备: {} (ID={})", device_selector::device_name(device_id), device_id);
    info!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");

    trigger::listen(
        hold_ms,
        // on_trigger: 开始录音
        {
            let is_rec = is_recording.clone();
            let audio_buf = audio_buffer.clone();
            move || {
                is_rec.store(true, Ordering::SeqCst);
                info!("🔴 Recording...");
                let is_rec = is_rec.clone();
                let audio_buf = audio_buf.clone();
                std::thread::spawn(move || {
                    let rec_cfg = recorder::RecorderConfig {
                        sample_rate,
                        device_id,
                        channels,
                    };
                    if let Err(e) = recorder::record_blocking(&rec_cfg, is_rec, &audio_buf) {
                        error!("Record error: {}", e);
                    }
                });
            }
        },
        // on_release: 停止录音 → ASR → LLM → 粘贴
        {
            let is_rec = is_recording.clone();
            let audio_buf = audio_buffer.clone();
            let tray = tray_mgr;
            let last_res = last_result.clone();
            move || {
                is_rec.store(false, Ordering::SeqCst);
                std::thread::sleep(std::time::Duration::from_millis(200));

                let audio_data = audio_buf.lock().unwrap().clone();
                if audio_data.is_empty() {
                    info!("⚠️  No audio data");
                    return;
                }
                let dur = audio_data.len() as f64 / sample_rate as f64;
                info!("📊 Audio: {:.1}s, {} samples", dur, audio_data.len());

                let raw = match &asr {
                    Some(engine) => engine.recognize(&audio_data, sample_rate)
                        .unwrap_or_else(|e| {
                            error!("ASR error: {}", e);
                            format!("[ASR Error: {}]", e)
                        }),
                    None => format!("[ASR占位-{:.1}s]", dur),
                };
                info!("📝 ASR: {}", raw);

                let final_text = match corrector.correct(&raw) {
                    Ok(t) => { info!("🔧 Corrected: {}", t); t }
                    Err(e) => { warn!("LLM failed: {}, using raw", e); raw }
                };

                tray.update_result(&final_text);
                if let Ok(mut lr) = last_res.lock() { *lr = final_text.clone(); }
                tray.show_notification("audio-input", &final_text);

                match paster.paste(&final_text) {
                    Ok(()) => info!("📋✅ Pasted: {}", final_text),
                    Err(e) => error!("Paste error: {}", e),
                }
                audio_buf.lock().unwrap().clear();
                info!("✅ Ready");
            }
        },
    );
}
