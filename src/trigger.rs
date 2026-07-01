//! 鼠标左键长按触发器
//! 按下瞬间 → on_press (提示音)
//! 按住3秒 → on_trigger (开始录音)
//! 松开 → on_release (停止录音+识别)

use log::{debug, info};
use std::sync::Arc;
use std::time::Instant;
use windows::Win32::UI::Input::KeyboardAndMouse::*;

const POLL_MS: u64 = 50;

pub fn listen<F1, F2, F3>(hold_ms: u64, on_press: F1, on_trigger: F2, on_release: F3)
where
    F1: Fn() + Send + 'static,
    F2: Fn() + Send + 'static,
    F3: Fn() + Send + 'static,
{
    let on_press = Arc::new(on_press);
    let on_trigger = Arc::new(on_trigger);
    let on_release = Arc::new(on_release);

    let mut pressed_at: Option<Instant> = None;
    let mut triggered = false;

    info!("🖱️  鼠标左键 {}ms 触发", hold_ms);

    loop {
        let is_down = unsafe {
            (GetAsyncKeyState(VK_LBUTTON.0 as i32) & 0x8000u16 as i16) != 0
        };

        match (is_down, pressed_at) {
            (true, None) => {
                // ★ 按下瞬间 — 播放提示音 (此时系统还未进入拖动状态)
                pressed_at = Some(Instant::now());
                triggered = false;
                debug!("左键按下");
                on_press();
            }
            (true, Some(start)) => {
                if !triggered && start.elapsed().as_millis() as u64 >= hold_ms {
                    triggered = true;
                    info!("🖱️✅ 触发录音 ({})ms", start.elapsed().as_millis());
                    on_trigger();
                }
            }
            (false, Some(start)) => {
                if triggered {
                    info!("🖱️⬆ 松开→识别流程");
                    on_release();
                } else {
                    debug!("短按{}ms忽略", start.elapsed().as_millis());
                }
                pressed_at = None;
                triggered = false;
            }
            (false, None) => {}
        }
        std::thread::sleep(std::time::Duration::from_millis(POLL_MS));
    }
}
