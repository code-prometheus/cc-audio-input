//! 系统光标 — 录音/修正中显示沙漏，完成恢复原箭头

use windows::Win32::Foundation::HANDLE;
use windows::Win32::UI::WindowsAndMessaging::*;

pub struct CursorManager;

impl CursorManager {
    pub fn set_recording() {
        set_sandglass();
    }
    pub fn set_thinking() {
        set_sandglass();
    }
    pub fn restore() {
        unsafe {
            if let Ok(arrow) = LoadCursorW(None, IDC_ARROW) {
                if let Ok(copy) = CopyImage(HANDLE(arrow.0), IMAGE_CURSOR, 0, 0, IMAGE_FLAGS(0)) {
                    let _ = SetSystemCursor(HCURSOR(copy.0), OCR_NORMAL);
                }
            }
            let _ = SystemParametersInfoW(SPI_SETCURSORS, 0, None, SPIF_SENDCHANGE);
        }
    }
}

fn set_sandglass() {
    unsafe {
        if let Ok(wait) = LoadCursorW(None, IDC_WAIT) {
            if let Ok(copy) = CopyImage(HANDLE(wait.0), IMAGE_CURSOR, 0, 0, IMAGE_FLAGS(0)) {
                let _ = SetSystemCursor(HCURSOR(copy.0), OCR_NORMAL);
            }
        }
    }
}
