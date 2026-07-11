//! 系统托盘 — tray-icon + winit (按经验文档方案)
//! 主线程: winit EventLoop + ApplicationHandler (菜单事件在 about_to_wait 中 try_recv)
//! 后台: 录音/ASR/LLM 业务通过 channel 通信
//! 图标: 程序化生成 黑底白色小a

use std::sync::{Arc, Mutex, mpsc};
use std::sync::atomic::{AtomicBool, Ordering};
use log::info;
use tray_icon::menu::{Menu, MenuEvent, MenuId, MenuItem, PredefinedMenuItem};
use tray_icon::{Icon, TrayIcon, TrayIconBuilder};
use winit::application::ApplicationHandler;
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::event::WindowEvent;
use winit::window::WindowId;

static mut G_LAST_RESULT: Option<Arc<Mutex<String>>> = None;
static mut G_TRIGGER_TX: Option<mpsc::Sender<()>> = None;
static mut G_RUNNING: Option<Arc<AtomicBool>> = None;

pub struct TrayManager {
    last_result: Arc<Mutex<String>>,
    running: Arc<AtomicBool>,
}

impl TrayManager {
    pub fn stub() -> Self {
        Self {
            last_result: Arc::new(Mutex::new(String::new())),
            running: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn update_result(&self, text: &str) {
        if let Ok(mut r) = self.last_result.lock() {
            *r = text.to_string();
        }
        unsafe { G_LAST_RESULT = Some(self.last_result.clone()); }
    }

    pub fn show_notification(&self, _title: &str, _body: &str) {
        info!("💬 {}: {}", _title, _body);
    }
}

/// 启动托盘并在主线程运行 event loop (阻塞)
/// 在 about_to_wait 中处理: 菜单事件 + 鼠标轮询 + trigger channel
pub fn run_tray_main<F1, F2>(
    tooltip: String,
    trigger_tx: mpsc::Sender<()>,
    trigger_rx: mpsc::Receiver<()>,
    input_devices: Vec<String>,
    active_input: usize,
    llm_models: Vec<String>,
    active_llm: usize,
    hold_ms: u64,
    on_trigger: F1,
    on_release: F2,
) where
    F1: Fn() + Send + 'static,
    F2: Fn() + Send + 'static,
{
    let last_result = Arc::new(Mutex::new(String::new()));
    let running = Arc::new(AtomicBool::new(true));

    unsafe {
        G_LAST_RESULT = Some(last_result.clone());
        G_TRIGGER_TX = Some(trigger_tx.clone());
        G_RUNNING = Some(running.clone());
    }

    // ★ 步骤1: 在 build() 之前获取 receiver
    let menu_rx = MenuEvent::receiver().clone();

    let Ok(event_loop) = EventLoop::new() else {
        info!("❌ 无法创建 EventLoop");
        return;
    };

    let icon = load_icon();
    let tray = TrayIconBuilder::new()
        .with_menu(Box::new(build_menu(&input_devices, active_input, &llm_models, active_llm)))
        .with_icon(icon)
        .with_tooltip(tooltip)
        .build()
        .ok();

    let mut app = TrayApp {
        tray,
        menu_rx,
        running: running.clone(),
        last_result,
        trigger_rx,
        trigger_tx,
        input_devices,
        active_input,
        llm_models,
        active_llm,
        hold_ms,
        on_trigger: Some(Arc::new(on_trigger)),
        on_release: Some(Arc::new(on_release)),
        is_pressed: false,
        press_start: None,
        press_position: None,
        mouse_move_threshold: 5.0,
    };

    let _ = event_loop.run_app(&mut app);
}

struct TrayApp<F1, F2>
where
    F1: Fn() + Send + 'static,
    F2: Fn() + Send + 'static,
{
    tray: Option<TrayIcon>,
    menu_rx: crossbeam_channel::Receiver<MenuEvent>,
    running: Arc<AtomicBool>,
    last_result: Arc<Mutex<String>>,
    trigger_rx: mpsc::Receiver<()>,
    trigger_tx: mpsc::Sender<()>,
    input_devices: Vec<String>,
    active_input: usize,
    llm_models: Vec<String>,
    active_llm: usize,
    hold_ms: u64,
    on_trigger: Option<Arc<F1>>,
    on_release: Option<Arc<F2>>,
    // 鼠标状态
    is_pressed: bool,
    press_start: Option<std::time::Instant>,
    press_position: Option<(f64, f64)>,
    mouse_move_threshold: f64,
}

impl<F1, F2> ApplicationHandler for TrayApp<F1, F2>
where
    F1: Fn() + Send + 'static,
    F2: Fn() + Send + 'static,
{
    fn resumed(&mut self, _event_loop: &ActiveEventLoop) {}
    fn window_event(&mut self, _event_loop: &ActiveEventLoop, _id: WindowId, _event: WindowEvent) {}

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        // ★ 核心: 在 about_to_wait 中 try_recv 菜单事件
        while let Ok(event) = self.menu_rx.try_recv() {
            self.handle_menu(&event.id.0);
        }

        // 检查手动触发 channel
        if self.trigger_rx.try_recv().is_ok() {
            self.do_trigger();
        }

        // ── 鼠标左键长按检测 ──
        self.poll_mouse();

        if !self.running.load(Ordering::SeqCst) {
            event_loop.exit();
        }
    }
}

impl<F1, F2> TrayApp<F1, F2>
where
    F1: Fn() + Send + 'static,
    F2: Fn() + Send + 'static,
{
    fn poll_mouse(&mut self) {
        use windows::Win32::UI::Input::KeyboardAndMouse::*;
        use windows::Win32::Foundation::POINT;
        use windows::Win32::UI::WindowsAndMessaging::GetCursorPos;

        let is_down = unsafe {
            (GetAsyncKeyState(VK_LBUTTON.0 as i32) & 0x8000u16 as i16) != 0
        };

        let mut cursor = POINT::default();
        let _ = unsafe { GetCursorPos(&mut cursor) };
        let cur_pos = (cursor.x as f64, cursor.y as f64);

        if is_down {
            if !self.is_pressed {
                // 刚按下
                self.is_pressed = true;
                self.press_start = Some(std::time::Instant::now());
                self.press_position = Some(cur_pos);
            } else {
                // 持续按住中
                if let (Some(start), Some(press_pos)) = (self.press_start, self.press_position) {
                    // 检查鼠标是否移动了
                    let dx = cur_pos.0 - press_pos.0;
                    let dy = cur_pos.1 - press_pos.1;
                    let moved = (dx * dx + dy * dy).sqrt();

                    if moved > self.mouse_move_threshold {
                        // 鼠标移动了 → 作废
                        log::debug!("鼠标移动 {:.1}px → 作废本次按下", moved);
                        self.is_pressed = false;
                        self.press_start = None;
                        self.press_position = None;
                        return;
                    }

                    if start.elapsed().as_millis() as u64 >= self.hold_ms {
                        // 按住不动达到阈值 → 触发!
                        self.is_pressed = false;
                        self.press_start = None;
                        self.press_position = None;
                        self.do_trigger();
                    }
                }
            }
        } else {
            // 鼠标松开 → 重置
            if self.is_pressed {
                let elapsed = self.press_start
                    .map(|s| s.elapsed().as_millis() as u64)
                    .unwrap_or(0);
                if elapsed > 50 {
                    log::debug!("短按{}ms忽略", elapsed);
                }
            }
            self.is_pressed = false;
            self.press_start = None;
            self.press_position = None;
        }
    }

    fn do_trigger(&self) {
        log::info!("🎤 触发录音");
        // 设置等待光标
        set_cursor_wait();
        if let Some(ref cb) = self.on_trigger {
            cb();
        }
        // 短暂等待后开始监听释放
        std::thread::sleep(std::time::Duration::from_millis(100));

        // 等待鼠标松开
        self.wait_for_release();
    }

    fn wait_for_release(&self) {
        use windows::Win32::UI::Input::KeyboardAndMouse::*;
        loop {
            let still_down = unsafe {
                (GetAsyncKeyState(VK_LBUTTON.0 as i32) & 0x8000u16 as i16) != 0
            };
            if !still_down {
                log::info!("🖱️⬆ 松开→识别流程");
                if let Some(ref cb) = self.on_release {
                    cb();
                }
                // 恢复光标
                set_cursor_arrow();
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
    }

    fn handle_menu(&mut self, id_str: &str) {
        match id_str {
            "__exit__" => {
                info!("👋 用户退出");
                std::process::exit(0);
            }
            "__record__" => {
                info!("🎤 托盘手动触发");
                let _ = self.trigger_tx.send(());
            }
            "__copy__" => {
                tray_copy();
            }
            id => {
                // 麦克风选择: mic_N
                if let Some(idx_str) = id.strip_prefix("mic_") {
                    if let Ok(idx) = idx_str.parse::<usize>() {
                        if idx < self.input_devices.len() {
                            info!("🎧 切换麦克风: {} (#{})", self.input_devices[idx], idx);
                            self.active_input = idx;
                            rebuild_menu_for(self);
                        }
                    }
                }
                // LLM 选择: llm_N
                if let Some(idx_str) = id.strip_prefix("llm_") {
                    if let Ok(idx) = idx_str.parse::<usize>() {
                        if idx < self.llm_models.len() {
                            info!("🤖 切换 LLM: {} (#{})", self.llm_models[idx], idx);
                            self.active_llm = idx;
                            rebuild_menu_for(self);
                        }
                    }
                }
            }
        }
    }
}

fn rebuild_menu_for<F1, F2>(app: &mut TrayApp<F1, F2>)
where
    F1: Fn() + Send + 'static,
    F2: Fn() + Send + 'static,
{
    if let Some(ref mut tray) = app.tray {
        let _ = tray.set_menu(Some(Box::new(build_menu(
            &app.input_devices,
            app.active_input,
            &app.llm_models,
            app.active_llm,
        ))));
        let _ = tray.set_tooltip(Some(format!(
            "audio-input 🎤 | LLM: {}",
            app.llm_models.get(app.active_llm).map(|s| s.as_str()).unwrap_or("?")
        )));
    }
}

fn build_menu(
    input_devices: &[String],
    active_input: usize,
    llm_models: &[String],
    active_llm: usize,
) -> Menu {
    let menu = Menu::new();

    // ── 状态显示 (disabled) ──
    let status = unsafe {
        G_LAST_RESULT.as_ref()
            .and_then(|lr| lr.lock().ok())
            .map(|r| if r.is_empty() {
                "📊 暂无结果".to_string()
            } else {
                let short: String = r.chars().take(30).collect();
                format!("📊 {}", short)
            })
            .unwrap_or_else(|| "📊 暂无结果".to_string())
    };
    menu.append(&MenuItem::with_id(
        MenuId::new("status".to_string()), status, false, None,
    )).ok();

    menu.append(&PredefinedMenuItem::separator()).ok();

    // ── 手动录音 ──
    menu.append(&MenuItem::with_id(
        MenuId::new("__record__".to_string()), "🎤 开始录音", true, None,
    )).ok();

    menu.append(&PredefinedMenuItem::separator()).ok();

    // ── 麦克风子菜单 ──
    let mic_sub = tray_icon::menu::Submenu::new("🎧 切换麦克风", true);
    for (i, dev) in input_devices.iter().enumerate() {
        let label = if i == active_input {
            format!("✓ {}", dev)
        } else {
            format!("  {}", dev)
        };
        mic_sub.append(&MenuItem::with_id(
            MenuId::new(format!("mic_{}", i)), label, true, None,
        )).ok();
    }
    menu.append(&mic_sub).ok();

    // ── LLM 模型子菜单 ──
    let llm_sub = tray_icon::menu::Submenu::new("🤖 LLM 模型", true);
    for (i, model) in llm_models.iter().enumerate() {
        let label = if i == active_llm {
            format!("✓ {}", model)
        } else {
            format!("  {}", model)
        };
        llm_sub.append(&MenuItem::with_id(
            MenuId::new(format!("llm_{}", i)), label, true, None,
        )).ok();
    }
    menu.append(&llm_sub).ok();

    menu.append(&PredefinedMenuItem::separator()).ok();

    // ── 拷贝 + 退出 ──
    menu.append(&MenuItem::with_id(
        MenuId::new("__copy__".to_string()), "📋 拷贝最后结果", true, None,
    )).ok();
    menu.append(&MenuItem::with_id(
        MenuId::new("__exit__".to_string()), "❌ 退出", true, None,
    )).ok();

    menu
}

fn load_icon() -> Icon {
    // 尝试从文件加载
    for p in &["assets/tray_icon.ico", "tray_icon.ico"] {
        let path = std::path::Path::new(p);
        if path.exists() {
            if let Ok(icon) = Icon::from_path(path, None) {
                return icon;
            }
        }
    }
    // 程序化生成: 黑底白色小a
    generate_a_icon()
}

/// 生成黑底白色小a图标 (32×32 RGBA)
fn generate_a_icon() -> Icon {
    let w = 32u32;
    let h = 32u32;
    let mut rgba = vec![0u8; (w * h * 4) as usize];
    let cx = 16.0;
    let cy = 16.0;
    let r = 14.0;

    for y in 0..h {
        for x in 0..w {
            let idx = ((y * w + x) * 4) as usize;
            let dx = x as f64 - cx;
            let dy = y as f64 - cy;
            if (dx * dx + dy * dy) <= r * r {
                // 黑色背景
                rgba[idx] = 30;
                rgba[idx + 1] = 30;
                rgba[idx + 2] = 30;
                rgba[idx + 3] = 255;
            } else {
                rgba[idx + 3] = 0; // 透明
            }
        }
    }

    // 简单画白色小写 a (14x10 居中)
    //   ##
    //  #  #
    // #    #
    // ######
    // #    #
    // #    #
    let a_pixels: &[(i32, i32)] = &[
        // a 的简单位图
        (12, 10), (13, 10),
        (11, 11), (14, 11),
        (10, 12), (15, 12),
        (10, 13), (11, 13), (12, 13), (13, 13), (14, 13), (15, 13),
        (10, 14), (15, 14),
        (10, 15), (15, 15),
    ];

    for &(ax, ay) in a_pixels {
        let px = ax as u32;
        let py = ay as u32;
        if px < w && py < h {
            let idx = ((py * w + px) * 4) as usize;
            rgba[idx] = 255;     // R
            rgba[idx + 1] = 255; // G
            rgba[idx + 2] = 255; // B
            rgba[idx + 3] = 255; // A
        }
    }

    Icon::from_rgba(rgba, w, h).expect("RGBA icon 生成失败")
}

fn tray_copy() {
    #[cfg(windows)]
    unsafe {
        use windows::Win32::System::DataExchange::*;
        use windows::Win32::System::Memory::*;
        use windows::Win32::Foundation::*;

        let text = G_LAST_RESULT
            .as_ref()
            .and_then(|lr| lr.lock().ok())
            .map(|r| r.clone())
            .unwrap_or_else(|| "(空)".to_string());
        let wide: Vec<u16> = text.encode_utf16().chain(std::iter::once(0)).collect();
        let size = wide.len() * 2;
        if let Ok(hmem) = GlobalAlloc(GMEM_MOVEABLE, size) {
            let ptr = GlobalLock(hmem);
            if !ptr.is_null() {
                std::ptr::copy_nonoverlapping(wide.as_ptr(), ptr as *mut u16, wide.len());
                let _ = GlobalUnlock(hmem);
                let _ = OpenClipboard(None);
                let _ = EmptyClipboard();
                let _ = SetClipboardData(13u32, HANDLE(hmem.0));
                let _ = CloseClipboard();
            }
        }
    }
}

/// 设置光标为等待 (沙漏)
fn set_cursor_wait() {
    #[cfg(windows)]
    unsafe {
        use windows::Win32::UI::WindowsAndMessaging::*;
        use windows::Win32::Foundation::HANDLE;
        if let Ok(wait) = LoadCursorW(None, IDC_WAIT) {
            if let Ok(copy) = CopyImage(HANDLE(wait.0), IMAGE_CURSOR, 0, 0, IMAGE_FLAGS(0)) {
                let _ = SetSystemCursor(HCURSOR(copy.0), OCR_NORMAL);
            }
        }
        let _ = SystemParametersInfoW(SPI_SETCURSORS, 0, None, SPIF_SENDCHANGE);
    }
}

/// 恢复光标为箭头
fn set_cursor_arrow() {
    #[cfg(windows)]
    unsafe {
        use windows::Win32::UI::WindowsAndMessaging::*;
        use windows::Win32::Foundation::HANDLE;
        if let Ok(arrow) = LoadCursorW(None, IDC_ARROW) {
            if let Ok(copy) = CopyImage(HANDLE(arrow.0), IMAGE_CURSOR, 0, 0, IMAGE_FLAGS(0)) {
                let _ = SetSystemCursor(HCURSOR(copy.0), OCR_NORMAL);
            }
        }
        let _ = SystemParametersInfoW(SPI_SETCURSORS, 0, None, SPIF_SENDCHANGE);
    }
}
