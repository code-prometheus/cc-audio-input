//! 鼠标左键长按触发器
//! 按住 1.5s 不动 → 触发录音 → 松开 → 识别
//! 如果在 1.5s 内鼠标移动了（拖拽/选区），重置等待

use log::{debug, info};
use std::sync::Arc;
use std::time::Instant;
use windows::Win32::UI::Input::KeyboardAndMouse::*;
use windows::Win32::UI::WindowsAndMessaging::GetCursorPos;
use windows::Win32::Foundation::POINT;

const POLL_MS: u64 = 50;
/// 鼠标移动阈值 (像素)，超过此距离视为拖动
const MOVE_THRESHOLD: i32 = 8;

fn cursor_pos() -> POINT {
    let mut pt = POINT { x: 0, y: 0 };
    unsafe { let _ = GetCursorPos(&mut pt); }
    pt
}

fn moved_enough(a: POINT, b: POINT) -> bool {
    let dx = (a.x - b.x).abs();
    let dy = (a.y - b.y).abs();
    (dx + dy) > MOVE_THRESHOLD
}

pub fn listen<F1, F2>(hold_ms: u64, on_trigger: F1, on_release: F2)
where
    F1: Fn() + Send + 'static,
    F2: Fn() + Send + 'static,
{
    let on_trigger = Arc::new(on_trigger);
    let on_release = Arc::new(on_release);

    info!("🖱️  鼠标左键 {}ms 触发", hold_ms);

    loop {
        // ── 阶段1: 等待鼠标按下 ──
        let is_down = unsafe {
            (GetAsyncKeyState(VK_LBUTTON.0 as i32) & 0x8000u16 as i16) != 0
        };
        if !is_down {
            std::thread::sleep(std::time::Duration::from_millis(POLL_MS));
            continue;
        }

        let press_time = Instant::now();
        let press_pos = cursor_pos();
        let mut triggered_ms = false;
        let mut moved = false;
        debug!("左键按下，等待 {}ms...", hold_ms);

        while unsafe {
            (GetAsyncKeyState(VK_LBUTTON.0 as i32) & 0x8000u16 as i16) != 0
        } {
            let elapsed = press_time.elapsed().as_millis() as u64;

            // ★ 检测鼠标移动: 如果动了超过阈值，取消触发
            if !triggered_ms && !moved {
                let cur = cursor_pos();
                if moved_enough(cur, press_pos) {
                    moved = true;
                    debug!("鼠标移动，取消触发");
                }
            }

            if moved {
                // 拖动模式: 等待松开后回到主循环重新开始
                if unsafe { (GetAsyncKeyState(VK_LBUTTON.0 as i32) & 0x8000u16 as i16) == 0 } {
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(POLL_MS));
                continue;
            }

            if !triggered_ms && elapsed >= hold_ms {
                triggered_ms = true;
                info!("🖱️✅ 触发录音 ({}ms)", elapsed);

                on_trigger();

                // ── 阶段2: 等待鼠标松开 ──
                std::thread::sleep(std::time::Duration::from_millis(100));

                loop {
                    let still_down = unsafe {
                        (GetAsyncKeyState(VK_LBUTTON.0 as i32) & 0x8000u16 as i16) != 0
                    };
                    if !still_down {
                        info!("🖱️⬆ 松开→识别流程");
                        on_release();
                        break;
                    }
                    std::thread::sleep(std::time::Duration::from_millis(50));
                }
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(POLL_MS));
        }

        if !triggered_ms && !moved {
            let elapsed = press_time.elapsed().as_millis() as u64;
            debug!("短按{}ms忽略", elapsed);
        }
    }
}
