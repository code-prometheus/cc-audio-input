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
        // on_press: 鼠标按下瞬间 → 提示音 (此时系统未进入拖动状态)
        {
            move || {
                std::thread::spawn(move || play_beep(output_id, 1800, 80));
            }
        },
        // on_trigger: 按住3秒 → 开始录音
        {
            let is_rec = is_recording.clone();
            let audio_buf = audio_buffer.clone();
            move || {
                is_rec.store(true, Ordering::SeqCst);
                info!("🔴 Recording...");
                let is_rec = is_rec.clone();
                let audio_buf = audio_buf.clone();
                std::thread::spawn(move || {
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

/// 通过 Win32 PlaySound 播放系统提示音 (绝对可靠)
fn play_beep(_output_device_id: i32, _freq: u32, _duration_ms: u64) {
    #[cfg(windows)]
    unsafe {
        use windows::Win32::Media::Audio::*;
        use windows::core::PCWSTR;

        let alias: Vec<u16> = "SystemAsterisk\0".encode_utf16().collect();
        PlaySoundW(
            PCWSTR::from_raw(alias.as_ptr()),
            None,
            SND_ALIAS | SND_ASYNC,
        );
    }
    #[cfg(not(windows))]
    {}
}
