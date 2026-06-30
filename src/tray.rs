//! 系统托盘 — Win32 Shell_NotifyIcon

use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicBool, Ordering};
use log::info;

pub struct TrayManager {
    last_result: Arc<Mutex<String>>,
    running: Arc<AtomicBool>,
}

impl TrayManager {
    pub fn create(tooltip: &str) -> Result<(Self, Arc<Mutex<String>>), String> {
        let last_result = Arc::new(Mutex::new(String::new()));
        let running = Arc::new(AtomicBool::new(true));
        let lr = last_result.clone();
        let run_val = running.clone();
        let tip = tooltip.to_string();

        info!("📌 托盘已创建: {}", &tip);

        std::thread::spawn(move || {
            run_tray(tip, run_val);
        });

        Ok((Self { last_result: lr, running: running }, last_result))
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
    }

    pub fn show_notification(&self, title: &str, body: &str) {
        let text = self.last_result.lock()
            .map(|r| r.clone())
            .unwrap_or_default();
        info!("💬 {}: {} (结果: {})", title, body, text);
    }
}

#[cfg(windows)]
fn run_tray(tooltip: String, running: Arc<AtomicBool>) {
    use windows::Win32::UI::Shell::*;
    use windows::Win32::UI::WindowsAndMessaging::*;
    use windows::Win32::Foundation::*;
    use windows::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows::core::PCWSTR;

    const WM_TRAYICON: u32 = WM_USER + 1;
    const ID_TRAY: u32 = 1;
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

        let mut nid = NOTIFYICONDATAW {
            cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32,
            hWnd: hwnd,
            uID: ID_TRAY,
            uFlags: NIF_MESSAGE | NIF_ICON | NIF_TIP,
            uCallbackMessage: WM_TRAYICON,
            hIcon: icon,
            szTip: tip_arr,
            ..Default::default()
        };

        Shell_NotifyIconW(NIM_ADD, &nid);

        let mut msg = MSG::default();
        while running.load(Ordering::SeqCst) {
            if GetMessageW(&mut msg, None, 0, 0).as_bool() {
                TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }

        Shell_NotifyIconW(NIM_DELETE, &nid);
        let _ = DestroyWindow(hwnd);
    }
}

#[cfg(windows)]
unsafe extern "system" fn tray_wndproc(
    hwnd: windows::Win32::Foundation::HWND,
    msg: u32,
    wparam: windows::Win32::Foundation::WPARAM,
    lparam: windows::Win32::Foundation::LPARAM,
) -> windows::Win32::Foundation::LRESULT {
    use windows::Win32::UI::WindowsAndMessaging::*;
    use windows::core::PCWSTR;

    const WM_TRAYICON: u32 = WM_USER + 1;
    const IDM_EXIT: usize = 101;

    if msg == WM_TRAYICON {
        let l = lparam.0 as u32;
        if l == WM_RBUTTONUP || l == WM_CONTEXTMENU {
            let mut pt = windows::Win32::Foundation::POINT::default();
            let _ = GetCursorPos(&mut pt);

            let menu = CreatePopupMenu().unwrap_or(HMENU(std::ptr::null_mut()));
            let exit_text: Vec<u16> = "❌ 退出\0".encode_utf16().collect();
            let _ = AppendMenuW(menu, MF_STRING, IDM_EXIT, PCWSTR::from_raw(exit_text.as_ptr()));
            let _ = SetForegroundWindow(hwnd);
            let _ = TrackPopupMenu(menu, TPM_BOTTOMALIGN | TPM_LEFTALIGN, pt.x, pt.y, 0, hwnd, None);
            let _ = DestroyMenu(menu);
        }
    } else if msg == WM_COMMAND && wparam.0 as usize == IDM_EXIT {
        PostQuitMessage(0);
    }

    DefWindowProcW(hwnd, msg, wparam, lparam)
}
