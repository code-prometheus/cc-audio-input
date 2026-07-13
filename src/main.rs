//! audio-input v0.5

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod config; mod trigger; mod recorder; mod asr_engine;
mod hotwords; mod corrector; mod clipboard_paste; mod device_selector; mod tray;

use log::{info, error, warn};
use std::sync::{Arc, Mutex, mpsc};
use std::sync::atomic::{AtomicBool, Ordering};

fn init_logging() {
    let exe_dir = std::env::current_exe().ok().and_then(|p| p.parent().map(|d| d.to_path_buf()))
        .unwrap_or_else(|| std::path::PathBuf::from("."));
    let log_path = exe_dir.join("audio-input.log");
    let _ = std::fs::write(&log_path, "");
    let file = std::fs::OpenOptions::new().create(true).write(true).open(&log_path).expect("log");
    let mut b = env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"));
    b.format(|buf, record| {
        use std::io::Write;
        writeln!(buf, "[{} {}] {}", chrono::Local::now().format("%H:%M:%S"), record.level(), record.args())
    }).target(env_logger::Target::Pipe(Box::new(file))).init();
}

fn restore_system_cursors() {
    #[cfg(windows)] unsafe {
        use windows::Win32::UI::WindowsAndMessaging::*;
        let _ = SystemParametersInfoW(SPI_SETCURSORS, 0, None, SPIF_SENDCHANGE);
    }
}

fn main() {
    restore_system_cursors(); // 启动时先恢复系统光标

    init_logging();
    info!("🚀 audio-input v0.5");
    let cfg = config::AppConfig::load();
    info!("✅ LLM: {} @ {}", cfg.llm.model, cfg.llm.base_url);
    let hw = hotwords::Hotwords::load(&std::path::PathBuf::from("hotwords.yaml")).expect("hotwords");
    let input_id = device_selector::resolve_input_device();
    info!("🎤 {}", device_selector::input_device_name(input_id));
    let asr = Arc::new(asr_engine::AsrEngine::new(&cfg.asr.model_dir).map(Some)
        .unwrap_or_else(|e| { warn!("ASR: {}", e); None }));
    let corrector = Arc::new(Mutex::new(corrector::Corrector::new(&cfg.llm, &hw).expect("LLM")));

    let input_devices: Vec<String> = device_selector::list_input_devices()
        .iter().map(|d| format!("{} ({}ch {}Hz)", d.name, d.channels, d.sample_rate)).collect();
    let llms: Vec<String> = cfg.llm_models.iter().map(|m| m.name.clone()).collect();
    let (trigger_tx, trigger_rx) = mpsc::channel::<()>();
    tray::run_tray_in_thread("audio-input 🎤".to_string(), trigger_tx, input_devices, 0, llms, cfg.active_llm_idx);

    let paster = clipboard_paste::ClipboardPaster::new();
    let is_rec = Arc::new(AtomicBool::new(false));
    let audio_buf: Arc<Mutex<Vec<f32>>> = Arc::new(Mutex::new(Vec::new()));
    let hold_ms = cfg.hotkey.hold_ms; let sr: u32 = 16000; let ch: u16 = 1;
    info!("🎤 Ready! {}s", hold_ms / 1000);

    let on_trigger = {
        let is_rec = is_rec.clone(); let audio_buf = audio_buf.clone();
        move || {
            tray::set_tooltip("🔴 录音中...");
            #[cfg(windows)] unsafe { use windows::Win32::UI::WindowsAndMessaging::*; use windows::Win32::Foundation::HANDLE; if let Ok(c) = LoadCursorW(None, IDC_WAIT) { if let Ok(co) = CopyImage(HANDLE(c.0), IMAGE_CURSOR, 0, 0, LR_COPYFROMRESOURCE) { let _ = SetSystemCursor(HCURSOR(co.0), OCR_NORMAL); } } }
            is_rec.store(true, Ordering::SeqCst);
            info!("🔴 Recording...");
            let is_rec = is_rec.clone(); let audio_buf = audio_buf.clone();
            std::thread::spawn(move || {
                #[cfg(windows)] unsafe {
                    let b = windows::Win32::System::Diagnostics::Debug::Beep;
                    b(2000, 100).ok(); std::thread::sleep(std::time::Duration::from_millis(80));
                    b(2000, 100).ok(); std::thread::sleep(std::time::Duration::from_millis(80));
                    b(2400, 150).ok();
                }
                let rc = recorder::RecorderConfig { sample_rate: sr, device_id: input_id, channels: ch };
                if let Err(e) = recorder::record_blocking(&rc, is_rec, &audio_buf) {
                    error!("Record: {}", e);
                }
            });
        }
    };

    let on_release = {
        let is_rec = is_rec.clone(); let audio_buf = audio_buf.clone();
        let corrector = corrector.clone(); let asr = asr.clone();
        move || {
            is_rec.store(false, Ordering::SeqCst);
            tray::set_tooltip("📝 语音识别中...");
            std::thread::sleep(std::time::Duration::from_millis(200));
            let data = audio_buf.lock().unwrap().clone();
            if data.is_empty() { info!("No audio"); tray::set_tooltip("audio-input 🎤"); restore_system_cursors(); return; }
            let dur = data.len() as f64 / sr as f64;
            info!("📊 {:.1}s", dur);

            let raw = match asr.as_ref().as_ref() {
                Some(e) => e.recognize(&data, sr).unwrap_or_else(|e| { error!("ASR: {}", e); format!("ERR") }),
                None => "N/A".to_string(),
            };
            info!("ASR {} chars", raw.len());
            tray::set_tooltip("🤖 LLM 修正中...");
            let text = match corrector.lock().unwrap().correct(&raw) {
                Ok(t) => { info!("LLM: {}", t); t }
                Err(e) => { warn!("LLM: {}", e); raw }
            };
            tray::set_last_result(&text);
            if !text.is_empty() && !text.starts_with('[') {
                let _ = paster.paste(&text);
            }
            audio_buf.lock().unwrap().clear();
            tray::set_tooltip("audio-input 🎤");
            restore_system_cursors(); // 恢复箭头
            info!("✅ Ready");
        }
    };

    let on_cancel = {
        let is_rec = is_rec.clone(); let audio_buf = audio_buf.clone();
        move || {
            is_rec.store(false, Ordering::SeqCst);
            info!("🚫 拖动取消, 不执行识别");
            audio_buf.lock().unwrap().clear();
            tray::set_tooltip("audio-input 🎤");
            restore_system_cursors();
        }
    };

    trigger::listen(hold_ms, trigger_rx, on_trigger, on_release, on_cancel);
}
