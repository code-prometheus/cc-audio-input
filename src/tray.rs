//! 系统托盘 — tray-icon + winit, 子线程 + tooltip channel + 切换通知

use std::sync::{Arc, Mutex, mpsc};
use std::sync::atomic::{AtomicBool, Ordering};
use log::info;
use tray_icon::menu::{Menu, MenuEvent, MenuId, MenuItem, PredefinedMenuItem};
use tray_icon::{Icon, TrayIcon, TrayIconBuilder};
use winit::application::ApplicationHandler;
use winit::event_loop::{ActiveEventLoop, ControlFlow};
use winit::platform::windows::EventLoopBuilderExtWindows;
use winit::event::WindowEvent;
use winit::window::WindowId;

static mut G_LAST_RESULT: Option<Arc<Mutex<String>>> = None;
static mut G_TOOLTIP_TX: Option<mpsc::Sender<String>> = None;
/// 托盘切换事件: (mic_index, llm_index) — None 表示未切换该项
pub static mut G_SWITCH_TX: Option<mpsc::Sender<(Option<usize>, Option<usize>)>> = None;

pub fn set_tooltip(text: &str) {
    log::info!("🔔 tooltip: {}", text);
    unsafe { if let Some(ref tx) = G_TOOLTIP_TX { let _ = tx.send(text.to_string()); } }
}

pub fn set_last_result(text: &str) {
    unsafe {
        if let Some(ref arc) = G_LAST_RESULT {
            if let Ok(mut s) = arc.lock() { *s = text.to_string(); }
        }
    }
}

pub fn run_tray_in_thread(
    tooltip: String,
    input_devices: Vec<String>, active_input: usize,
    llm_models: Vec<String>, active_llm: usize,
    switch_tx: mpsc::Sender<(Option<usize>, Option<usize>)>,
) {
    let (tooltip_tx, tooltip_rx) = mpsc::channel::<String>();
    unsafe { G_TOOLTIP_TX = Some(tooltip_tx); G_LAST_RESULT = Some(Arc::new(Mutex::new(String::new()))); G_SWITCH_TX = Some(switch_tx); }

    let menu_rx = MenuEvent::receiver().clone();
    let running = Arc::new(AtomicBool::new(true));

    std::thread::spawn(move || {
        let Ok(event_loop) = winit::event_loop::EventLoop::builder().with_any_thread(true).build() else { return };
        let tray = TrayIconBuilder::new()
            .with_menu(Box::new(build_menu(&input_devices, active_input, &llm_models, active_llm)))
            .with_icon(load_icon()).with_tooltip(tooltip).build().ok();
        let mut app = TrayApp { tray, menu_rx, tooltip_rx, running, input_devices, active_input, llm_models, active_llm };
        let _ = event_loop.run_app(&mut app);
    });
}

struct TrayApp {
    tray: Option<TrayIcon>, menu_rx: crossbeam_channel::Receiver<MenuEvent>,
    tooltip_rx: mpsc::Receiver<String>, running: Arc<AtomicBool>,
    input_devices: Vec<String>, active_input: usize,
    llm_models: Vec<String>, active_llm: usize,
}

impl ApplicationHandler for TrayApp {
    fn resumed(&mut self, _e: &ActiveEventLoop) {}
    fn window_event(&mut self, _e: &ActiveEventLoop, _id: WindowId, _event: WindowEvent) {}
    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        event_loop.set_control_flow(ControlFlow::Poll);
        while let Ok(event) = self.menu_rx.try_recv() { self.handle_menu(&event.id.0); }
        if let Ok(tt) = self.tooltip_rx.try_recv() {
            if let Some(ref t) = self.tray { let _ = t.set_tooltip(Some(tt)); }
        }
        if !self.running.load(Ordering::SeqCst) { event_loop.exit(); }
    }
}

impl TrayApp {
    fn handle_menu(&mut self, id_str: &str) {
        match id_str {
            "__exit__" => { info!("👋 退出"); std::process::exit(0); }
            "__copy__" => tray_copy(),
            id => {
                if let Some(s) = id.strip_prefix("mic_") {
                    if let Ok(i) = s.parse::<usize>() {
                        if i < self.input_devices.len() {
                            self.active_input = i;
                            rebuild_menu(self);
                            // 通知主线程切换麦克风
                            unsafe { if let Some(ref tx) = G_SWITCH_TX { let _ = tx.send((Some(i), None)); } }
                        }
                    }
                }
                if let Some(s) = id.strip_prefix("llm_") {
                    if let Ok(i) = s.parse::<usize>() {
                        if i < self.llm_models.len() {
                            self.active_llm = i;
                            rebuild_menu(self);
                            // 通知主线程切换 LLM 模型
                            unsafe { if let Some(ref tx) = G_SWITCH_TX { let _ = tx.send((None, Some(i))); } }
                        }
                    }
                }
            }
        }
    }
}

fn rebuild_menu(app: &mut TrayApp) {
    if let Some(ref mut t) = app.tray {
        let _ = t.set_menu(Some(Box::new(build_menu(&app.input_devices, app.active_input, &app.llm_models, app.active_llm))));
    }
}

fn build_menu(devs: &[String], ai: usize, llms: &[String], al: usize) -> Menu {
    let m = Menu::new();
    let sm = tray_icon::menu::Submenu::new("🎧 切换麦克风", true);
    for (i, d) in devs.iter().enumerate() {
        sm.append(&MenuItem::with_id(MenuId::new(format!("mic_{}", i)), if i == ai { format!("✓ {}", d) } else { format!("  {}", d) }, true, None)).ok();
    }
    m.append(&sm).ok();
    let sl = tray_icon::menu::Submenu::new("🤖 LLM 模型", true);
    for (i, l) in llms.iter().enumerate() {
        sl.append(&MenuItem::with_id(MenuId::new(format!("llm_{}", i)), if i == al { format!("✓ {}", l) } else { format!("  {}", l) }, true, None)).ok();
    }
    m.append(&sl).ok();
    m.append(&PredefinedMenuItem::separator()).ok();
    m.append(&MenuItem::with_id(MenuId::new("__copy__".to_string()), "📋 拷贝", true, None)).ok();
    m.append(&MenuItem::with_id(MenuId::new("__exit__".to_string()), "❌ 退出", true, None)).ok();
    m
}

fn load_icon() -> Icon {
    for p in &["assets/tray_icon.ico", "tray_icon.ico"] {
        if std::path::Path::new(p).exists() {
            if let Ok(i) = Icon::from_path(std::path::Path::new(p), None) { return i; }
        }
    }
    generate_a_icon()
}

fn generate_a_icon() -> Icon {
    let (w, h) = (32u32, 32u32);
    let mut rgba = vec![0u8; (w * h * 4) as usize];
    for y in 0..h { for x in 0..w {
        if ((x as f64 - 16.0).powi(2) + (y as f64 - 16.0).powi(2)) <= 196.0 {
            let idx = ((y * w + x) * 4) as usize;
            rgba[idx] = 35; rgba[idx+1] = 35; rgba[idx+2] = 35; rgba[idx+3] = 255;
        }
    }}
    let a: &[(i32,i32)] = &[
        (11,10),(12,10),(13,10),(14,10),(15,10),(16,10),(17,10),(18,10),(19,10),(20,10),
        (10,11),(21,11),(9,12),(22,12),
        (9,13),(10,13),(11,13),(12,13),(13,13),(14,13),(15,13),(16,13),(17,13),(18,13),(19,13),(20,13),(21,13),(22,13),
        (9,14),(22,14),(9,15),(22,15),(10,16),(21,16),
        (11,17),(12,17),(13,17),(14,17),(15,17),(16,17),(17,17),(18,17),(19,17),(20,17),
    ];
    for &(x, y) in a {
        if x >= 0 && y >= 0 && (x as u32) < w && (y as u32) < h {
            let idx = ((y as u32 * w + x as u32) * 4) as usize;
            rgba[idx]=255; rgba[idx+1]=255; rgba[idx+2]=255; rgba[idx+3]=255;
        }
    }
    Icon::from_rgba(rgba, w, h).expect("icon")
}

fn tray_copy() {
    #[cfg(windows)] unsafe {
        use windows::Win32::System::DataExchange::*;
        use windows::Win32::System::Memory::*;
        let text = G_LAST_RESULT.as_ref().and_then(|r| r.lock().ok()).map(|s| s.clone()).unwrap_or_default();
        let w: Vec<u16> = text.encode_utf16().chain(std::iter::once(0)).collect();
        if let Ok(h) = GlobalAlloc(GMEM_MOVEABLE, w.len() * 2) {
            let p = GlobalLock(h);
            if !p.is_null() { std::ptr::copy_nonoverlapping(w.as_ptr(), p as *mut u16, w.len()); let _ = GlobalUnlock(h); }
            let _ = OpenClipboard(None); let _ = EmptyClipboard();
            let _ = SetClipboardData(13u32, windows::Win32::Foundation::HANDLE(h.0));
            let _ = CloseClipboard();
        }
    }
}
