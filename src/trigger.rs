//! 录音触发器 — 鼠标左键长按 + 托盘手动触发
//! 按住1.5秒 → 释放检测 → 播放提示音 + 开始录音 → 重新监测松开 → 停止+识别
//! 同时也通过 mpsc channel 接收托盘菜单的手动触发请求

use log::{debug, info};
use std::sync::Arc;
use std::sync::mpsc;
use std::time::{Duration, Instant};
use windows::Win32::UI::Input::KeyboardAndMouse::*;

const POLL_MS: u64 = 50;

pub fn listen<F1, F2>(hold_ms: u64, trigger_rx: mpsc::Receiver<()>, on_trigger: F1, on_release: F2)
where
    F1: Fn() + Send + 'static,
    F2: Fn() + Send + 'static,
{
    let on_trigger = Arc::new(on_trigger);
    let on_release = Arc::new(on_release);

    info!("🖱️ 鼠标左键 {}ms 触发", hold_ms);

    loop {
        // ── 阶段1: 等待鼠标按下并持续 hold_ms，或等待手动触发 ──
        let triggered = wait_for_trigger(hold_ms, &trigger_rx);
        if !triggered {
            continue;
        }

        info!("🎤 触发录音");

        // 播放提示音 + 开始录音
        on_trigger();

        // 短暂休息让系统处理
        std::thread::sleep(Duration::from_millis(100));

        // ── 阶段2: 等待鼠标松开 ──
        loop {
            let still_down = unsafe {
                (GetAsyncKeyState(VK_LBUTTON.0 as i32) & 0x8000u16 as i16) != 0
            };
            if !still_down {
                info!("🖱️⬆ 松开→识别流程");
                on_release();
                break;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
    }
}

/// 等待触发信号：鼠标长按 或 托盘手动触发
fn wait_for_trigger(hold_ms: u64, rx: &mpsc::Receiver<()>) -> bool {
    loop {
        // 检查手动触发
        if rx.try_recv().is_ok() {
            info!("🖱️✅ 手动触发 (托盘菜单)");
            return true;
        }

        let is_down = unsafe {
            (GetAsyncKeyState(VK_LBUTTON.0 as i32) & 0x8000u16 as i16) != 0
        };

        if !is_down {
            std::thread::sleep(Duration::from_millis(POLL_MS));
            continue;
        }

        let press_time = Instant::now();
        debug!("左键按下，等待 {}ms...", hold_ms);

        // 轮询等待：要么按时触发，要么短按取消
        loop {
            if rx.try_recv().is_ok() {
                info!("🖱️✅ 手动触发 (托盘菜单，鼠标按住中)");
                return true;
            }

            let still_down = unsafe {
                (GetAsyncKeyState(VK_LBUTTON.0 as i32) & 0x8000u16 as i16) != 0
            };

            if !still_down {
                debug!("短按{}ms忽略", press_time.elapsed().as_millis());
                return false; // 短按，回到外层循环
            }

            if press_time.elapsed().as_millis() as u64 >= hold_ms {
                return true; // 长按触发
            }

            std::thread::sleep(Duration::from_millis(POLL_MS));
        }
    }
}
