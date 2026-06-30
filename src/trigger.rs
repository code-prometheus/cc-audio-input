//! 鼠标左键长按触发器

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

    let mut pressed_at: Option<Instant> = None;
    let mut triggered = false;

    info!("🖱️  鼠标左键 {}ms 触发", hold_ms);

    loop {
        let is_down = unsafe {
            (GetAsyncKeyState(VK_LBUTTON.0 as i32) & 0x8000u16 as i16) != 0
        };

        match (is_down, pressed_at) {
            (true, None) => {
                pressed_at = Some(Instant::now());
                triggered = false;
                debug!("左键按下");
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
