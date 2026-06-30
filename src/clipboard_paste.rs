//! 剪贴板 + Ctrl+V 粘贴 — Win32 API

use anyhow::Result;
use log::{info, debug};
use windows::Win32::System::DataExchange::*;
use windows::Win32::System::Memory::*;
use windows::Win32::Foundation::*;
use windows::Win32::UI::Input::KeyboardAndMouse::*;

pub struct ClipboardPaster;

impl ClipboardPaster {
    pub fn new() -> Self { Self }

    fn write_clipboard(&self, text: &str) -> Result<()> {
        unsafe {
            if !OpenClipboard(None).is_ok() {
                return Err(anyhow::anyhow!("无法打开剪贴板"));
            }
            if !EmptyClipboard().is_ok() {
                let _ = CloseClipboard();
                return Err(anyhow::anyhow!("无法清空剪贴板"));
            }

            let wide: Vec<u16> = text.encode_utf16().chain(std::iter::once(0)).collect();
            let byte_size = wide.len() * 2;

            let h_mem = GlobalAlloc(
                GMEM_MOVEABLE,
                byte_size,
            ).map_err(|_| anyhow::anyhow!("GlobalAlloc失败"))?;

            let ptr = GlobalLock(h_mem);
            if ptr.is_null() {
                let _ = GlobalFree(h_mem);
                let _ = CloseClipboard();
                return Err(anyhow::anyhow!("GlobalLock失败"));
            }

            std::ptr::copy_nonoverlapping(wide.as_ptr(), ptr as *mut u16, wide.len());
            let _ = GlobalUnlock(h_mem);

            // CF_UNICODETEXT = 13
            let result = SetClipboardData(13, HANDLE(h_mem.0));
            if result.is_err() {
                let _ = GlobalFree(h_mem);
                let _ = CloseClipboard();
                return Err(anyhow::anyhow!("SetClipboardData失败"));
            }
            let _ = CloseClipboard();
        }
        debug!("📋 剪贴板: {}...", &text[..text.len().min(50)]);
        Ok(())
    }

    fn simulate_paste(&self) {
        unsafe {
            std::thread::sleep(std::time::Duration::from_millis(50));
            keybd_event(VK_CONTROL.0 as u8, 0, KEYEVENTF_EXTENDEDKEY, 0);
            keybd_event(b'V', 0, KEYEVENTF_EXTENDEDKEY, 0);
            std::thread::sleep(std::time::Duration::from_millis(30));
            keybd_event(b'V', 0, KEYEVENTF_KEYUP | KEYEVENTF_EXTENDEDKEY, 0);
            keybd_event(VK_CONTROL.0 as u8, 0, KEYEVENTF_KEYUP | KEYEVENTF_EXTENDEDKEY, 0);
        }
        debug!("⌨️  Ctrl+V");
    }

    pub fn paste(&self, text: &str) -> Result<()> {
        if text.is_empty() {
            info!("⚠️  空文本跳过");
            return Ok(());
        }
        self.write_clipboard(text)?;
        self.simulate_paste();
        info!("📋✅ 已粘贴");
        Ok(())
    }
}
