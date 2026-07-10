//! 系统托盘 — tray-icon + winit (按 RUST_TRAY_EXPERIENCE.md 经验)
//!
//! 主线程: winit EventLoop + ApplicationHandler
//! 后台: 录音/ASR/LLM 业务线程
//! 菜单事件: about_to_wait + try_recv (唯一可行方案)

use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicBool, Ordering};
use log::info;
use tray_icon::menu::{Menu, MenuEvent, MenuId, MenuItem};
use tray_icon::{Icon, TrayIcon, TrayIconBuilder};
use winit::application::ApplicationHandler;
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::event::WindowEvent;
use winit::window::WindowId;
use winit::platform::windows::EventLoopBuilderExtWindows;

static mut G_LAST_RESULT: Option<Arc<Mutex<String>>> = None;

pub struct TrayManager {
    last_result: Arc<Mutex<String>>,
    running: Arc<AtomicBool>,
}

impl TrayManager {
    pub fn create(tooltip: &str) -> Result<(Self, Arc<Mutex<String>>), String> {
        let last_result = Arc::new(Mutex::new(String::new()));
        let running = Arc::new(AtomicBool::new(true));
        let lr = last_result.clone();
        let run = running.clone();
        let tip = tooltip.to_string();

        unsafe { G_LAST_RESULT = Some(lr.clone()); }

        // ★ 必须在 build 之前获取 receiver (经验文档 4.2 步骤1)
        let menu_rx = MenuEvent::receiver().clone();

        // 后台线程: 托盘 event loop
        std::thread::spawn(move || run_tray(tip, menu_rx, run));

        info!("📌 托盘已创建: {}", tooltip);
        Ok((Self { last_result: lr, running }, last_result))
    }

    pub fn stub() -> Self {
        Self {
            last_result: Arc::new(Mutex::new(String::new())),
            running: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn update_result(&self, text: &str) {
        if let Ok(mut r) = self.last_result.lock() { *r = text.to_string(); }
    }

    pub fn show_notification(&self, title: &str, body: &str) {
        info!("💬 {}: {}", title, body);
    }
}

fn load_icon() -> Icon {
    // 从文件加载 (经验文档: from_path/from_rgba)
    let paths = [
        std::path::PathBuf::from("assets/tray_icon.ico"),
        std::path::PathBuf::from("tray_icon.ico"),
    ];
    let exe_dir = std::env::current_exe().ok()
        .and_then(|p| p.parent().map(|d| d.to_path_buf()));
    let search: Vec<std::path::PathBuf> = exe_dir.iter()
        .flat_map(|d| vec![d.join("tray_icon.ico")])
        .chain(paths)
        .collect();

    for p in &search {
        if p.exists() {
            if let Ok(icon) = Icon::from_path(p, None) {
                info!("✅ 图标加载: {}", p.display());
                return icon;
            }
        }
    }

    // Fallback: 程序化黄底圆
    info!("⚠️  图标文件未找到, 使用程序化生成");
    let (w, h) = (32u32, 32u32);
    let mut rgba = vec![0u8; (w * h * 4) as usize];
    let cx = 16.0f32; let cy = 16.0f32; let r = 14.0f32;
    for y in 0..h {
        for x in 0..w {
            let dx = x as f32 - cx + 0.5;
            let dy = y as f32 - cy + 0.5;
            let idx = ((y * w + x) * 4) as usize;
            if (dx * dx + dy * dy) <= r * r {
                rgba[idx] = 255;     // R
                rgba[idx + 1] = 215; // G
                rgba[idx + 2] = 0;   // B
                rgba[idx + 3] = 255; // A
            }
        }
    }
    Icon::from_rgba(rgba, w, h).unwrap()
}

fn build_menu() -> Menu {
    let menu = Menu::new();
    // ★ with_id 绑定菜单 ID (经验文档: 必须用 with_id)
    menu.append(&MenuItem::with_id(MenuId::new("copy".to_string()), "📋 拷贝最后结果", true, None)).ok();
    menu.append(&MenuItem::with_id(MenuId::new("exit".to_string()), "❌ 退出", true, None)).ok();
    menu
}

fn run_tray(tooltip: String, menu_rx: crossbeam_channel::Receiver<MenuEvent>, running: Arc<AtomicBool>) {
    // ★ 用 any_thread 允许非主线程创建 EventLoop (tray-icon + winit 要求)
    let Ok(event_loop) = winit::event_loop::EventLoopBuilder::new()
        .with_any_thread(true)
        .build() else {
        info!("❌ 无法创建 EventLoop");
        return;
    };

    let icon = load_icon();
    let tray = TrayIconBuilder::new()
        .with_tooltip(tooltip)
        .with_icon(icon)
        .with_menu(Box::new(build_menu()))
        .build()
        .ok();

    let mut app = TrayApp {
        tray,
        menu_rx,
        running: running.clone(),
    };

    if event_loop.run_app(&mut app).is_err() {
        info!("托盘退出");
    }
    running.store(false, Ordering::SeqCst);
}

struct TrayApp {
    tray: Option<TrayIcon>,
    menu_rx: crossbeam_channel::Receiver<MenuEvent>,
    running: Arc<AtomicBool>,
}

impl ApplicationHandler for TrayApp {
    fn resumed(&mut self, _event_loop: &ActiveEventLoop) {}

    fn window_event(&mut self, _event_loop: &ActiveEventLoop, _id: WindowId, _event: WindowEvent) {}

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        // ★ 经验文档核心: 在这里 try_recv 是唯一能收到菜单事件的方式
        while let Ok(event) = self.menu_rx.try_recv() {
            handle_menu_id(&event.id.0, self.tray.as_mut());
        }

        if !self.running.load(Ordering::SeqCst) {
            event_loop.exit();
        }
    }
}

fn handle_menu_id(id_str: &str, _tray: Option<&mut TrayIcon>) {
    match id_str {
        "copy" => tray_copy(),
        "exit" => {
            info!("👋 用户退出");
            std::process::exit(0);
        }
        _ => {}
    }
}

fn tray_copy() {
    #[cfg(windows)]
    unsafe {
        let text = G_LAST_RESULT.as_ref()
            .and_then(|lr| lr.lock().ok())
            .map(|r| r.clone())
            .unwrap_or_else(|| "(空)".to_string());
        let wide: Vec<u16> = text.encode_utf16().chain(std::iter::once(0)).collect();
        let size = wide.len() * 2;
        if let Ok(hmem) = windows::Win32::System::Memory::GlobalAlloc(
            windows::Win32::System::Memory::GMEM_MOVEABLE, size,
        ) {
            let ptr = windows::Win32::System::Memory::GlobalLock(hmem);
            if !ptr.is_null() {
                std::ptr::copy_nonoverlapping(wide.as_ptr(), ptr as *mut u16, wide.len());
                windows::Win32::System::Memory::GlobalUnlock(hmem);
                windows::Win32::System::DataExchange::OpenClipboard(None).ok();
                windows::Win32::System::DataExchange::EmptyClipboard().ok();
                windows::Win32::System::DataExchange::SetClipboardData(
                    13u32, windows::Win32::Foundation::HANDLE(hmem.0),
                ).ok();
                windows::Win32::System::DataExchange::CloseClipboard();
            }
        }
    }
}
