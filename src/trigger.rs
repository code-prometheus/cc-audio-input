//! 录音触发器 — 鼠标左键长按 + 托盘手动触发 + 拖动检测

use log::{debug, info};
use std::sync::Arc;
use std::sync::mpsc;
use std::time::{Duration, Instant};
use windows::Win32::Foundation::POINT;
use windows::Win32::UI::Input::KeyboardAndMouse::*;
use windows::Win32::UI::WindowsAndMessaging::GetCursorPos;

const POLL_MS: u64 = 50;
const DRAG_THRESHOLD: i32 = 8; // 像素, 区分抖动和有意拖动

pub fn listen<F1, F2, F3>(hold_ms: u64, trigger_rx: mpsc::Receiver<()>, on_trigger: F1, on_release: F2, on_cancel: F3)
where
    F1: Fn() + Send + 'static,
    F2: Fn() + Send + 'static,
    F3: Fn() + Send + 'static,
{
    let on_trigger = Arc::new(on_trigger);
    let on_release = Arc::new(on_release);
    let on_cancel = Arc::new(on_cancel);
    info!("🖱️ 鼠标左键 {}ms 触发 (拖动阈值 {}px)", hold_ms, DRAG_THRESHOLD);

    loop {
        let triggered = wait_for_trigger(hold_ms, &trigger_rx);
        if !triggered { continue; }
        info!("🎤 触发录音");
        on_trigger();
        std::thread::sleep(Duration::from_millis(100));

        // 阶段2: 等待鼠标松开, 每帧维持等待光标 + 拖动检测
        let anchor_x;
        let anchor_y;
        unsafe { let mut p = POINT::default(); let _ = GetCursorPos(&mut p); anchor_x = p.x; anchor_y = p.y; }
        loop {
            let still_down = unsafe {
                (GetAsyncKeyState(VK_LBUTTON.0 as i32) & 0x8000u16 as i16) != 0
            };
            if !still_down {
                info!("🖱️⬆ 松开→识别流程");
                on_release();
                break;
            }
            // 拖动检测
            unsafe {
                let mut p = POINT::default();
                let _ = GetCursorPos(&mut p);
                if (p.x - anchor_x).abs() > DRAG_THRESHOLD || (p.y - anchor_y).abs() > DRAG_THRESHOLD {
                    debug!("拖动取消 (dx={}, dy={})", p.x - anchor_x, p.y - anchor_y);
                    on_cancel();
                    break;
                }
            }
            #[cfg(windows)] unsafe {
                use windows::Win32::UI::WindowsAndMessaging::*;
                if let Ok(c) = LoadCursorW(None, IDC_WAIT) { SetCursor(HCURSOR(c.0)); }
            }
            std::thread::sleep(Duration::from_millis(50));
        }
    }
}

fn wait_for_trigger(hold_ms: u64, rx: &mpsc::Receiver<()>) -> bool {
    loop {
        if rx.try_recv().is_ok() { info!("🖱️✅ 手动触发"); return true; }
        let is_down = unsafe { (GetAsyncKeyState(VK_LBUTTON.0 as i32) & 0x8000u16 as i16) != 0 };
        if !is_down { std::thread::sleep(Duration::from_millis(POLL_MS)); continue; }
        let press_time = Instant::now();
        // 记录按下时的位置
        let anchor_x;
        let anchor_y;
        unsafe { let mut p = POINT::default(); let _ = GetCursorPos(&mut p); anchor_x = p.x; anchor_y = p.y; }
        debug!("左键按下 ({}，{})，等待 {}ms...", anchor_x, anchor_y, hold_ms);
        loop {
            if rx.try_recv().is_ok() { info!("🖱️✅ 手动触发"); return true; }
            let still_down = unsafe { (GetAsyncKeyState(VK_LBUTTON.0 as i32) & 0x8000u16 as i16) != 0 };
            if !still_down { debug!("短按{}ms忽略", press_time.elapsed().as_millis()); return false; }
            // 拖动检测
            unsafe {
                let mut p = POINT::default();
                let _ = GetCursorPos(&mut p);
                if (p.x - anchor_x).abs() > DRAG_THRESHOLD || (p.y - anchor_y).abs() > DRAG_THRESHOLD {
                    debug!("拖动取消 (dx={}, dy={})", p.x - anchor_x, p.y - anchor_y);
                    return false;
                }
            }
            if press_time.elapsed().as_millis() as u64 >= hold_ms { return true; }
            std::thread::sleep(Duration::from_millis(POLL_MS));
        }
    }
}
