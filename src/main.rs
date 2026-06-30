//! audio-input v0.2
//! Hold left mouse button 3s → record → ASR → LLM correct → Ctrl+V paste

mod config;
mod trigger;
mod recorder;
mod asr_engine;
mod hotwords;
mod corrector;
mod clipboard_paste;

use log::{info, error, warn};
use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicBool, Ordering};

fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
        .format_timestamp_millis()
        .init();

    info!("🚀 audio-input v0.2");

    let cfg = config::AppConfig::load();
    info!("✅ LLM: {} @ {}", cfg.llm.model, cfg.llm.base_url);

    let hw = hotwords::Hotwords::load(
        &std::path::PathBuf::from("assets/hotwords.yaml")
    ).expect("Failed to load hotwords.yaml");
    info!("✅ Hotwords: {} words, {} phonetic pairs", hw.word_count(), hw.phonetic_count());

    let is_recording = Arc::new(AtomicBool::new(false));
    let audio_buffer: Arc<Mutex<Vec<f32>>> = Arc::new(Mutex::new(Vec::new()));

    let asr = asr_engine::AsrEngine::new_placeholder();
    let corrector = corrector::Corrector::new(&cfg.llm, &hw)
        .expect("Failed to create LLM corrector");
    info!("✅ LLM corrector ready");

    let paster = clipboard_paste::ClipboardPaster::new();
    let hold_ms = cfg.hotkey.hold_ms;

    info!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    info!("🎤 Ready! Hold left mouse button {}s to record", hold_ms / 1000);
    info!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");

    trigger::listen(
        hold_ms,
        // on_trigger: start recording
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
                        sample_rate: 16000,
                        device_id: -1,
                        channels: 1,
                    };
                    if let Err(e) = recorder::record_blocking(&rec_cfg, is_rec, &audio_buf) {
                        error!("Record error: {}", e);
                    }
                });
            }
        },
        // on_release: stop → ASR → LLM → paste
        {
            let is_rec = is_recording.clone();
            let audio_buf = audio_buffer.clone();
            move || {
                is_rec.store(false, Ordering::SeqCst);
                std::thread::sleep(std::time::Duration::from_millis(200));

                let audio_data = audio_buf.lock().unwrap().clone();
                if audio_data.is_empty() {
                    info!("⚠️  No audio data");
                    return;
                }
                let dur = audio_data.len() as f64 / 16000.0;
                info!("📊 Audio: {:.1}s", dur);

                let raw = asr.recognize(&audio_data, 16000).unwrap_or_else(|e| {
                    error!("ASR: {}", e);
                    "[ASR Error]".to_string()
                });
                info!("📝 ASR: {}", raw);

                let final_text = match corrector.correct(&raw) {
                    Ok(t) => { info!("🔧 Corrected: {}", t); t }
                    Err(e) => { warn!("LLM correction failed: {}, using raw", e); raw }
                };

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
