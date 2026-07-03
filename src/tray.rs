//! 系统托盘 — Shell_NotifyIconW
//! 托盘图标 + 右键(拷贝/退出) + show_notification 更新 tooltip

use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicBool, Ordering};
use log::info;

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

        std::thread::spawn(move || run_tray(tip, run));

        info!("📌 托盘已创建: {}", tooltip);
        Ok((Self { last_result: lr, running }, last_result))
    }

    pub fn stub() -> Self {
        Self { last_result: Arc::new(Mutex::new(String::new())), running: Arc::new(AtomicBool::new(false)) }
    }

    pub fn update_result(&self, text: &str) {
        if let Ok(mut r) = self.last_result.lock() { *r = text.to_string(); }
        unsafe { G_LAST_RESULT = Some(self.last_result.clone()); }
    }

    pub fn show_notification(&self, _title: &str, _body: &str) {
        info!("💬 {}: {}", _title, _body);
    }
}

#[cfg(windows)]
fn run_tray(tooltip: String, running: Arc<AtomicBool>) {
    use windows::Win32::UI::Shell::*;
    use windows::Win32::UI::WindowsAndMessaging::*;
    use windows::Win32::Foundation::*;
    use windows::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows::core::PCWSTR;
    use windows::Win32::System::DataExchange::*;
    use windows::Win32::System::Memory::*;

    const WM_TRAYICON: u32 = WM_USER + 1;
    const ID_TRAY: u32 = 1;
    const IDM_COPY: usize = 100;
    const IDM_EXIT: usize = 101;

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
        let Ok(hwnd) = CreateWindowExW(WINDOW_EX_STYLE::default(), PCWSTR::from_raw(cn.as_ptr()), PCWSTR::from_raw(cn.as_ptr()), WS_OVERLAPPED, 0, 0, 0, 0, None, None, hinstance, None) else { return };

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

        let mut msg = MSG::default();
        while running.load(Ordering::SeqCst) {
            if GetMessageW(&mut msg, None, 0, 0).as_bool() {
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }

        let _ = Shell_NotifyIconW(NIM_DELETE, &nid);
        let _ = DestroyWindow(hwnd);
    }
}

#[cfg(windows)]
unsafe extern "system" fn tray_wndproc(hwnd: windows::Win32::Foundation::HWND, msg: u32, wparam: windows::Win32::Foundation::WPARAM, lparam: windows::Win32::Foundation::LPARAM) -> windows::Win32::Foundation::LRESULT {
    use windows::Win32::UI::WindowsAndMessaging::*;
    use windows::Win32::UI::Shell::*;
    use windows::Win32::System::DataExchange::*;
    use windows::Win32::System::Memory::*;
    use windows::core::PCWSTR;

    const WM_TRAYICON: u32 = WM_USER + 1;
    const IDM_COPY: usize = 100;
    const IDM_EXIT: usize = 101;

    if msg == WM_TRAYICON {
        let l = lparam.0 as u32;
        if l == WM_RBUTTONUP || l == WM_CONTEXTMENU {
            let mut pt = windows::Win32::Foundation::POINT::default();
            let _ = GetCursorPos(&mut pt);
            let menu = CreatePopupMenu().unwrap_or(HMENU(std::ptr::null_mut()));
            let copy_text: Vec<u16> = "📋 拷贝最后结果\0".encode_utf16().collect();
            let exit_text: Vec<u16> = "❌ 退出\0".encode_utf16().collect();
            let _ = AppendMenuW(menu, MF_STRING, IDM_COPY, PCWSTR::from_raw(copy_text.as_ptr()));
            let _ = AppendMenuW(menu, MF_STRING, IDM_EXIT, PCWSTR::from_raw(exit_text.as_ptr()));
            let _ = SetForegroundWindow(hwnd);
            let _ = TrackPopupMenu(menu, TPM_BOTTOMALIGN | TPM_LEFTALIGN, pt.x, pt.y, 0, hwnd, None);
            let _ = DestroyMenu(menu);
        } else if l == WM_LBUTTONDBLCLK {
            tray_copy();
        }
    } else if msg == WM_COMMAND {
        match wparam.0 as usize {
            IDM_COPY => tray_copy(),
            IDM_EXIT => PostQuitMessage(0),
            _ => {}
        }
    }
    DefWindowProcW(hwnd, msg, wparam, lparam)
}

unsafe fn tray_copy() {
    let text = G_LAST_RESULT.as_ref().and_then(|lr| lr.lock().ok()).map(|r| r.clone()).unwrap_or_else(|| "(empty)".to_string());
    let wide: Vec<u16> = text.encode_utf16().chain(std::iter::once(0)).collect();
    let size = wide.len() * 2;
    if let Ok(hmem) = windows::Win32::System::Memory::GlobalAlloc(windows::Win32::System::Memory::GMEM_MOVEABLE, size) {
        let ptr = windows::Win32::System::Memory::GlobalLock(hmem);
        if !ptr.is_null() {
            std::ptr::copy_nonoverlapping(wide.as_ptr(), ptr as *mut u16, wide.len());
            windows::Win32::System::Memory::GlobalUnlock(hmem);
            windows::Win32::System::DataExchange::OpenClipboard(None).ok();
            windows::Win32::System::DataExchange::EmptyClipboard().ok();
            windows::Win32::System::DataExchange::SetClipboardData(13u32, windows::Win32::Foundation::HANDLE(hmem.0)).ok();
            windows::Win32::System::DataExchange::CloseClipboard();
        }
    }
}
