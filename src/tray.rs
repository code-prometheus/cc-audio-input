//! 系统托盘 — 增强版 Win32 Shell_NotifyIconW
//! - 多级菜单：状态/录音/设备/LLM模型/拷贝/退出
//! - 手动录音触发通道
//! - 动态菜单更新

use std::sync::{Arc, Mutex, mpsc};
use std::sync::atomic::{AtomicBool, Ordering};
use log::info;

static mut G_LAST_RESULT: Option<Arc<Mutex<String>>> = None;
static mut G_TRIGGER_TX: Option<mpsc::Sender<()>> = None;

pub struct TrayManager {
    last_result: Arc<Mutex<String>>,
    running: Arc<AtomicBool>,
}

/// 菜单项 ID 常量
mod menu_ids {
    pub const ID_STATUS: usize = 200;
    pub const ID_SEP1: usize = 300;
    pub const ID_RECORD: usize = 400;
    pub const ID_SEP2: usize = 500;
    pub const ID_MIC_SUBMENU: usize = 600;
    pub const ID_MIC_BASE: usize = 601; // 601+idx
    pub const ID_LLM_SUBMENU: usize = 700;
    pub const ID_LLM_BASE: usize = 701; // 701+idx
    pub const ID_SEP3: usize = 800;
    pub const ID_COPY: usize = 900;
    pub const ID_EXIT: usize = 901;
}

impl TrayManager {
    pub fn create(
        tooltip: &str,
        trigger_tx: mpsc::Sender<()>,
        input_devices: Vec<String>,
        active_input: usize,
        llm_models: Vec<String>,
        active_llm: usize,
    ) -> Result<(Self, Arc<Mutex<String>>), String> {
        let last_result = Arc::new(Mutex::new(String::new()));
        let running = Arc::new(AtomicBool::new(true));
        let lr = last_result.clone();
        let run = running.clone();
        let tip = tooltip.to_string();

        unsafe {
            G_LAST_RESULT = Some(lr.clone());
            G_TRIGGER_TX = Some(trigger_tx.clone());
        }

        std::thread::spawn(move || {
            run_tray(tip, run, trigger_tx, input_devices, active_input, llm_models, active_llm);
        });

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
        if let Ok(mut r) = self.last_result.lock() {
            *r = text.to_string();
        }
        unsafe { G_LAST_RESULT = Some(self.last_result.clone()); }
    }

    pub fn show_notification(&self, _title: &str, _body: &str) {
        info!("💬 {}: {}", _title, _body);
    }
}

#[cfg(windows)]
fn run_tray(
    tooltip: String,
    running: Arc<AtomicBool>,
    trigger_tx: mpsc::Sender<()>,
    input_devices: Vec<String>,
    active_input: usize,
    llm_models: Vec<String>,
    active_llm: usize,
) {
    use windows::Win32::UI::Shell::*;
    use windows::Win32::UI::WindowsAndMessaging::*;
    use windows::Win32::Foundation::*;
    use windows::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows::core::PCWSTR;

    const WM_TRAYICON: u32 = WM_USER + 1;
    const ID_TRAY: u32 = 1;

    unsafe {
        let Ok(hinstance) = GetModuleHandleW(None) else { return };
        let cn: Vec<u16> = "AITrayCls\0".encode_utf16().collect();
        let wc = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            lpfnWndProc: Some(tray_wndproc),
            hInstance: hinstance.into(),
            lpszClassName: PCWSTR::from_raw(cn.as_ptr()),
            ..Default::default()
        };
        RegisterClassExW(&wc);
        let Ok(hwnd) = CreateWindowExW(
            WINDOW_EX_STYLE::default(),
            PCWSTR::from_raw(cn.as_ptr()),
            PCWSTR::from_raw(cn.as_ptr()),
            WS_OVERLAPPED,
            0, 0, 0, 0,
            None, None, hinstance, None,
        ) else { return };

        let Ok(icon) = LoadIconW(None, IDI_APPLICATION) else { return };
        let tip_wide: Vec<u16> = tooltip.encode_utf16().take(127).chain(std::iter::once(0)).collect();
        let mut tip_arr = [0u16; 128];
        let n = tip_wide.len().min(127);
        tip_arr[..n].copy_from_slice(&tip_wide[..n]);

        let nid = NOTIFYICONDATAW {
            cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32,
            hWnd: hwnd,
            uID: ID_TRAY,
            uFlags: NIF_MESSAGE | NIF_ICON | NIF_TIP,
            uCallbackMessage: WM_TRAYICON,
            hIcon: icon,
            szTip: tip_arr,
            ..Default::default()
        };
        let _ = Shell_NotifyIconW(NIM_ADD, &nid);

        // 将菜单数据存储到窗口属性中
        let menu_data = Box::new(TrayMenuData {
            trigger_tx,
            input_devices,
            active_input,
            llm_models,
            active_llm,
        });
        SetWindowLongPtrW(hwnd, GWLP_USERDATA, Box::into_raw(menu_data) as isize);

        let mut msg = MSG::default();
        while running.load(Ordering::SeqCst) {
            if GetMessageW(&mut msg, None, 0, 0).as_bool() {
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }

        // 清理
        let ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut TrayMenuData;
        if !ptr.is_null() {
            drop(Box::from_raw(ptr));
        }
        let _ = Shell_NotifyIconW(NIM_DELETE, &nid);
        let _ = DestroyWindow(hwnd);
    }
}

struct TrayMenuData {
    trigger_tx: mpsc::Sender<()>,
    input_devices: Vec<String>,
    active_input: usize,
    llm_models: Vec<String>,
    active_llm: usize,
}

#[cfg(windows)]
unsafe extern "system" fn tray_wndproc(
    hwnd: windows::Win32::Foundation::HWND,
    msg: u32,
    wparam: windows::Win32::Foundation::WPARAM,
    lparam: windows::Win32::Foundation::LPARAM,
) -> windows::Win32::Foundation::LRESULT {
    use windows::Win32::UI::WindowsAndMessaging::*;
    use windows::Win32::Foundation::POINT;

    const WM_TRAYICON: u32 = WM_USER + 1;

    if msg == WM_TRAYICON {
        let l = lparam.0 as u32;
        if l == WM_RBUTTONUP || l == WM_CONTEXTMENU {
            let mut pt = POINT::default();
            let _ = GetCursorPos(&mut pt);

            let menu_data_ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut TrayMenuData;
            let menu = build_tray_menu(menu_data_ptr);
            let _ = SetForegroundWindow(hwnd);
            let _ = TrackPopupMenu(
                menu,
                TPM_BOTTOMALIGN | TPM_LEFTALIGN,
                pt.x, pt.y, 0, hwnd, None,
            );
            // 菜单会在 WM_COMMAND 处理后销毁
        } else if l == WM_LBUTTONDBLCLK {
            // 双击 = 手动触发录音
            tray_trigger(hwnd);
        }
    } else if msg == WM_COMMAND {
        let id = wparam.0 as usize;
        handle_menu_action(hwnd, id);
    }

    DefWindowProcW(hwnd, msg, wparam, lparam)
}

#[cfg(windows)]
unsafe fn build_tray_menu(menu_data_ptr: *mut TrayMenuData) -> windows::Win32::UI::WindowsAndMessaging::HMENU {
    use windows::Win32::UI::WindowsAndMessaging::*;
    use windows::core::PCWSTR;

    use crate::tray::menu_ids::*;

    let menu = CreatePopupMenu().unwrap_or(HMENU(std::ptr::null_mut()));

    // ── 状态显示 ──
    let status_text = unsafe {
        G_LAST_RESULT
            .as_ref()
            .and_then(|lr| lr.lock().ok())
            .map(|r| {
                if r.is_empty() {
                    "📊 暂无识别结果".to_string()
                } else {
                    let short: String = r.chars().take(40).collect();
                    format!("📊 最后结果: {}", short)
                }
            })
            .unwrap_or_else(|| "📊 暂无识别结果".to_string())
    };
    let status_wide: Vec<u16> = status_text.encode_utf16().chain(std::iter::once(0)).collect();
    let _ = AppendMenuW(menu, MF_STRING | MF_GRAYED, ID_STATUS, PCWSTR::from_raw(status_wide.as_ptr()));

    // ── 分隔符 ──
    let sep1: Vec<u16> = "────────────────\0".encode_utf16().collect();
    let _ = AppendMenuW(menu, MF_SEPARATOR, ID_SEP1, PCWSTR::from_raw(sep1.as_ptr()));

    // ── 手动录音 ──
    let rec_text: Vec<u16> = "🎤 开始录音 (双击托盘也可触发)\0".encode_utf16().collect();
    let _ = AppendMenuW(menu, MF_STRING, ID_RECORD, PCWSTR::from_raw(rec_text.as_ptr()));

    // ── 分隔符 ──
    let sep2: Vec<u16> = "────────────────\0".encode_utf16().collect();
    let _ = AppendMenuW(menu, MF_SEPARATOR, ID_SEP2, PCWSTR::from_raw(sep2.as_ptr()));

    // ── 麦克风子菜单 ──
    if !menu_data_ptr.is_null() {
        let data = &*menu_data_ptr;
        let mic_menu = CreatePopupMenu().unwrap_or(HMENU(std::ptr::null_mut()));
        for (i, dev) in data.input_devices.iter().enumerate() {
            let label = if i == data.active_input {
                format!("✓ {}", dev)
            } else {
                format!("  {}", dev)
            };
            let label_wide: Vec<u16> = label.encode_utf16().chain(std::iter::once(0)).collect();
            let _ = AppendMenuW(mic_menu, MF_STRING, ID_MIC_BASE + i, PCWSTR::from_raw(label_wide.as_ptr()));
        }
        let mic_text: Vec<u16> = "🎧 切换麦克风\0".encode_utf16().collect();
        let _ = AppendMenuW(menu, MF_POPUP, mic_menu.0 as usize, PCWSTR::from_raw(mic_text.as_ptr()));

        // ── LLM 模型子菜单 ──
        let llm_menu = CreatePopupMenu().unwrap_or(HMENU(std::ptr::null_mut()));
        for (i, model) in data.llm_models.iter().enumerate() {
            let label = if i == data.active_llm {
                format!("✓ {}", model)
            } else {
                format!("  {}", model)
            };
            let label_wide: Vec<u16> = label.encode_utf16().chain(std::iter::once(0)).collect();
            let _ = AppendMenuW(llm_menu, MF_STRING, ID_LLM_BASE + i, PCWSTR::from_raw(label_wide.as_ptr()));
        }
        let llm_text: Vec<u16> = "🤖 LLM 模型\0".encode_utf16().collect();
        let _ = AppendMenuW(menu, MF_POPUP, llm_menu.0 as usize, PCWSTR::from_raw(llm_text.as_ptr()));
    }

    // ── 分隔符 ──
    let sep3: Vec<u16> = "────────────────\0".encode_utf16().collect();
    let _ = AppendMenuW(menu, MF_SEPARATOR, ID_SEP3, PCWSTR::from_raw(sep3.as_ptr()));

    // ── 拷贝 + 退出 ──
    let copy_text: Vec<u16> = "📋 拷贝最后结果\0".encode_utf16().collect();
    let _ = AppendMenuW(menu, MF_STRING, ID_COPY, PCWSTR::from_raw(copy_text.as_ptr()));
    let exit_text: Vec<u16> = "❌ 退出\0".encode_utf16().collect();
    let _ = AppendMenuW(menu, MF_STRING, ID_EXIT, PCWSTR::from_raw(exit_text.as_ptr()));

    menu
}

#[cfg(windows)]
unsafe fn handle_menu_action(hwnd: windows::Win32::Foundation::HWND, id: usize) {
    use crate::tray::menu_ids::*;
    use windows::Win32::UI::WindowsAndMessaging::*;

    match id {
        ID_RECORD => {
            tray_trigger(hwnd);
        }
        ID_COPY => {
            tray_copy();
        }
        ID_EXIT => {
            PostQuitMessage(0);
        }
        _ => {
            // 检查麦克风选择
            let data_ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut TrayMenuData;
            if !data_ptr.is_null() {
                let data = &*data_ptr;
                if id >= ID_MIC_BASE && id < ID_MIC_BASE + data.input_devices.len() {
                    let idx = id - ID_MIC_BASE;
                    info!("🎧 切换麦克风: {} (#{})", data.input_devices[idx], idx);
                    // TODO: 实际切换需要通知 main.rs 重建设备
                } else if id >= ID_LLM_BASE && id < ID_LLM_BASE + data.llm_models.len() {
                    let idx = id - ID_LLM_BASE;
                    info!("🤖 切换 LLM: {} (#{})", data.llm_models[idx], idx);
                    // TODO: 实际切换需要通知 main.rs 重建 corrector
                }
            }
        }
    }
}

unsafe fn tray_trigger(_hwnd: windows::Win32::Foundation::HWND) {
    info!("🎤 托盘触发录音");
    if let Some(ref tx) = G_TRIGGER_TX {
        let _ = tx.send(());
    }
}

unsafe fn tray_copy() {
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
            GlobalUnlock(hmem);
            OpenClipboard(None).ok();
            EmptyClipboard().ok();
            SetClipboardData(13u32, HANDLE(hmem.0)).ok();
            CloseClipboard();
        }
    }
}
