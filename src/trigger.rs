//! 鼠标左键长按触发器
//! 按住3秒 → 释放检测 → 播放提示音 + 开始录音 → 重新监测松开 → 停止+识别

use log::{debug, info};
use std::sync::Arc;
use std::time::Instant;
use windows::Win32::UI::Input::KeyboardAndMouse::*;

const POLL_MS: u64 = 50;

pub fn listen<F1, F2>(hold_ms: u64, on_trigger: F1, on_release: F2)
where
    F1: Fn() + Send + 'static,
    F2: Fn() + Send + 'static,
{
    let on_trigger = Arc::new(on_trigger);
    let on_release = Arc::new(on_release);

    info!("🖱️  鼠标左键 {}ms 触发", hold_ms);

    loop {
        // ── 阶段1: 等待鼠标按下并持续 hold_ms ──
        let is_down = unsafe {
            (GetAsyncKeyState(VK_LBUTTON.0 as i32) & 0x8000u16 as i16) != 0
        };
        if !is_down {
            std::thread::sleep(std::time::Duration::from_millis(POLL_MS));
            continue;
        }

        let press_time = Instant::now();
        let mut triggered_ms = false;
        debug!("左键按下，等待 {}ms...", hold_ms);

        while unsafe {
            (GetAsyncKeyState(VK_LBUTTON.0 as i32) & 0x8000u16 as i16) != 0
        } {
            let elapsed = press_time.elapsed().as_millis() as u64;
            if !triggered_ms && elapsed >= hold_ms {
                triggered_ms = true;
                info!("🖱️✅ 触发录音 ({}ms)", elapsed);

                // ★ 释放检测循环，让鼠标恢复自由
                // ★ 播放提示音 + 开始录音
                on_trigger();

                // ── 阶段2: 等待鼠标松开 ──
                // 短暂休息让系统处理
                std::thread::sleep(std::time::Duration::from_millis(100));

                // 轮询等待鼠标松开（鼠标已经自由了）
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
                // 松开后回到外层主循环等待下一次按下
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(POLL_MS));
        }

        // 如果短按（没触发3秒），跳过
        if !triggered_ms {
            let elapsed = press_time.elapsed().as_millis() as u64;
            debug!("短按{}ms忽略", elapsed);
        }
    }
}
