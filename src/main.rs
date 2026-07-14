//! audio-input v0.7

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod config; mod trigger; mod recorder; mod asr_engine;
mod hotwords; mod corrector; mod clipboard_paste; mod device_selector; mod tray;

use log::{info, error, warn};
use std::sync::{Arc, Mutex, mpsc};
use std::sync::atomic::{AtomicBool, AtomicI32, Ordering};

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

fn is_valid_text(text: &str) -> bool {
    let t = text.trim();
    if t.is_empty() { return false; }
    if t == "." || t == "。" { return false; }
    if t == "<|nospeech|>" { return false; }
    if t.chars().all(|c| c.is_ascii_punctuation()
        || c == '。' || c == '，' || c == '、' || c == ' '
        || c == '\t' || c == '\n' || c == '\r')
    {
        return false;
    }
    true
}

fn main() {
    restore_system_cursors();
    init_logging();
    info!("🚀 audio-input v0.7");
    let cfg = config::AppConfig::load();
    info!("✅ LLM: {} @ {}", cfg.llm.model, cfg.llm.base_url);
    let hw = hotwords::Hotwords::load(&std::path::PathBuf::from("hotwords.yaml")).expect("hotwords");

    let input_id = Arc::new(AtomicI32::new(device_selector::resolve_input_device()));
    info!("🎤 {}", device_selector::input_device_name(input_id.load(Ordering::SeqCst)));

    // ASR 引擎初始化 — 模型缺失时弹 Windows 对话框并退出
    let asr = match asr_engine::AsrEngine::new(&cfg.asr.model_dir) {
        Ok(engine) => Arc::new(Some(engine)),
        Err(e) => {
            let msg = format!(
                "SenseVoice ASR 模型未找到!\n\n\
                 需要: {}\\model.int8.onnx\n\
                 需要: {}\\tokens.txt\n\n\
                 下载地址:\n\
                 https://github.com/k2-fsa/sherpa-onnx/releases/download/asr-models/sherpa-onnx-sense-voice-zh-en-ja-ko-yue-int8-2025-09-09.tar.bz2\n\n\
                 下载后解压到 exe 同目录的 models/sense-voice-int8/ 文件夹\n\n\
                 程序即将退出。",
                cfg.asr.model_dir.display(), cfg.asr.model_dir.display()
            );
            #[cfg(windows)] unsafe {
                use windows::Win32::UI::WindowsAndMessaging::*;
                use windows::core::PCWSTR;
                let title: Vec<u16> = "audio-input\0".encode_utf16().collect();
                let body: Vec<u16> = msg.encode_utf16().chain(std::iter::once(0)).collect();
                MessageBoxW(None, PCWSTR(body.as_ptr()), PCWSTR(title.as_ptr()), MB_OK | MB_ICONERROR);
            }
            error!("{}", e);
            std::process::exit(1);
        }
    };
    let corrector: Arc<Mutex<corrector::Corrector>> = Arc::new(Mutex::new(
        corrector::Corrector::new(&cfg.llm, &hw).expect("LLM")));

    let input_devices: Vec<String> = device_selector::list_input_devices()
        .iter().map(|d| format!("{} ({}ch {}Hz)", d.name, d.channels, d.sample_rate)).collect();
    let (switch_tx, switch_rx) = mpsc::channel::<Option<usize>>();
    tray::run_tray_in_thread(
        "audio-input 🎤".to_string(),
        input_devices, 0,
        switch_tx,
    );

    let paster = clipboard_paste::ClipboardPaster::new();
    let is_rec = Arc::new(AtomicBool::new(false));
    let audio_buf: Arc<Mutex<Vec<f32>>> = Arc::new(Mutex::new(Vec::new()));
    let hold_ms = cfg.hotkey.hold_ms; let sr: u32 = 16000; let ch: u16 = 1;
    info!("🎤 Ready! {}s", hold_ms / 1000);

    let on_trigger = {
        let is_rec = is_rec.clone(); let audio_buf = audio_buf.clone();
        let input_id = input_id.clone();
        move || {
            tray::set_tooltip("🔴 录音中...");
            #[cfg(windows)] unsafe { use windows::Win32::UI::WindowsAndMessaging::*; use windows::Win32::Foundation::HANDLE; if let Ok(c) = LoadCursorW(None, IDC_WAIT) { if let Ok(co) = CopyImage(HANDLE(c.0), IMAGE_CURSOR, 0, 0, LR_COPYFROMRESOURCE) { let _ = SetSystemCursor(HCURSOR(co.0), OCR_NORMAL); } } }
            is_rec.store(true, Ordering::SeqCst);
            info!("🔴 Recording...");
            let is_rec = is_rec.clone(); let audio_buf = audio_buf.clone();
            let dev_id = input_id.load(Ordering::SeqCst);
            std::thread::spawn(move || {
                #[cfg(windows)] unsafe {
                    let b = windows::Win32::System::Diagnostics::Debug::Beep;
                    b(2000, 100).ok(); std::thread::sleep(std::time::Duration::from_millis(80));
                    b(2000, 100).ok(); std::thread::sleep(std::time::Duration::from_millis(80));
                    b(2400, 150).ok();
                }
                let rc = recorder::RecorderConfig { sample_rate: sr, device_id: dev_id, channels: ch };
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
            if data.is_empty() {
                info!("No audio");
                tray::set_tooltip("audio-input 🎤");
                restore_system_cursors();
                return;
            }
            let dur = data.len() as f64 / sr as f64;
            info!("📊 {:.1}s", dur);

            let raw = match asr.as_ref().as_ref() {
                Some(e) => e.recognize(&data, sr).unwrap_or_else(|e| { error!("ASR: {}", e); format!("ERR") }),
                None => "N/A".to_string(),
            };
            info!("ASR: {} chars", raw.len());

            if !is_valid_text(&raw) {
                info!("ASR 无有效语音内容, 跳过修正: '{}'", raw.chars().take(80).collect::<String>());
                tray::set_last_result("");
                audio_buf.lock().unwrap().clear();
                tray::set_tooltip("audio-input 🎤");
                restore_system_cursors();
                info!("✅ Ready");
                return;
            }

            tray::set_tooltip("🤖 LLM 修正中...");
            // 最多重试3次：如果结果和生文本一样，立即重试
            let mut text = raw.clone();
            let max_retries = 3;
            for attempt in 0..max_retries {
                match corrector.lock().unwrap().correct(&raw) {
                    Ok(t) => {
                        if is_valid_text(&t) {
                            info!("LLM (第{}次): {}", attempt + 1, t);
                            text = t;
                            // 和生文本不一样 → 成功，退出重试
                            if text.trim() != raw.trim() { break; }
                            info!("🔄 LLM 结果与生文本一致，立即重试 ({}/{})", attempt + 1, max_retries);
                        } else {
                            info!("LLM 返回无效内容 (第{}次): '{}'", attempt + 1, t.chars().take(80).collect::<String>());
                            break;
                        }
                    }
                    Err(e) => {
                        warn!("LLM (第{}次): {}", attempt + 1, e);
                        break;
                    }
                }
            }

            // 如果和生文本完全一样（修正失败/无变化），末尾打标注
            let final_text = if text.trim() == raw.trim() {
                if text.trim().is_empty() { text } else { format!("{} (生文本)", text.trim()) }
            } else {
                text
            };

            tray::set_last_result(&final_text);
            if is_valid_text(&final_text) {
                let _ = paster.paste(&final_text);
                info!("📋✅ 已粘贴");
            } else {
                info!("⏭️ 无有效指令, 不粘贴");
            }
            audio_buf.lock().unwrap().clear();
            tray::set_tooltip("audio-input 🎤");
            restore_system_cursors();
            info!("✅ Ready");
        }
    };

    let on_cancel = {
        let is_rec = is_rec.clone(); let audio_buf = audio_buf.clone();
        move || {
            is_rec.store(false, Ordering::SeqCst);
            info!("🚫 拖动取消, 不执行识别");
            std::thread::sleep(std::time::Duration::from_millis(100));
            audio_buf.lock().unwrap().clear();
            tray::set_tooltip("audio-input 🎤");
            restore_system_cursors();
        }
    };

    // 托盘麦克风切换监听线程
    {
        let input_id = input_id.clone();
        std::thread::spawn(move || {
            for mic_idx in switch_rx {
                if let Some(i) = mic_idx {
                    let devs = device_selector::list_input_devices();
                    if let Some(d) = devs.get(i) {
                        info!("🔄 切换麦克风: [{}] {}", i, d.name);
                        input_id.store(d.id as i32, Ordering::SeqCst);
                    }
                }
            }
        });
    }

    let (_dummy_tx, dummy_rx) = mpsc::channel::<()>();
    trigger::listen(hold_ms, dummy_rx, on_trigger, on_release, on_cancel);
}