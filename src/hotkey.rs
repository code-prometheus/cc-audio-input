//! 全局热键监听模块
//!
//! 实现 Push-To-Talk (PTT) 模式：
//! - 按下热键 → 触发 on_press 回调（开始录音）
//! - 松开热键 → 触发 on_release 回调（停止录音→ASR→LLM→粘贴）
//!
//! 使用 Win32 API: RegisterHotKey + GetMessage (MSG 循环中处理 WM_HOTKEY)
//! 结合 GetAsyncKeyState 轮询检测按键松开（因为 RegisterHotKey 只报告按下）

use anyhow::{Context, Result};
use log::{error, info, debug};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use windows::core::*;
use windows::Win32::UI::Input::KeyboardAndMouse::*;
use windows::Win32::UI::WindowsAndMessaging::*;

const MOD_NOREPEAT: u32 = 0x4000;  // 禁止热键自动重复

/// 轮询间隔 (ms)
const POLL_INTERVAL_MS: u64 = 10;

/// 开始监听全局热键
///
/// # 参数
/// - `vk_code`: 虚拟键码（如 0x71 = F2）
/// - `on_press`: 按键按下时的回调
/// - `on_release`: 按键松开时的回调
///
/// # 行为
/// 此函数会阻塞当前线程，在 Windows 消息循环中监听热键。
/// 建议在独立线程中调用。
pub fn listen<F1, F2>(vk_code: u32, on_press: F1, on_release: F2) -> Result<()>
where
    F1: Fn() + Send + 'static,
    F2: Fn() + Send + 'static,
{
    let vk = VIRTUAL_KEY(vk_code as u16);

    // ── 注册全局热键 ──
    // 热键 ID 用 1，修饰键只加 MOD_NOREPEAT（不要求 Ctrl/Alt）
    unsafe {
        let result = RegisterHotKey(
            None,           // NULL = 当前线程
            1,              // 热键 ID
            HOT_KEY_MODIFIERS(MOD_NOREPEAT),
            vk.0 as u32,
        );
        if result.is_err() {
            let err = windows::core::Error::from_win32();
            return Err(anyhow::anyhow!(
                "全局热键注册失败 (vk_code=0x{:X}): {}. 可能被其他应用占用或需要管理员权限。",
                vk_code,
                err
            ));
        }
    }
    info!("🔑 热键已注册: VK 0x{:02X}", vk_code);

    // ── 按键状态跟踪 ──
    let was_pressed = Arc::new(AtomicBool::new(false));
    let press_fired = Arc::new(AtomicBool::new(false));

    let on_press = Arc::new(on_press);
    let on_release = Arc::new(on_release);

    // ── Windows 消息循环 ──
    // 需要消息循环来接收 WM_HOTKEY
    // 同时轮询 GetAsyncKeyState 检测松开

    let mut msg = MSG::default();

    loop {
        // 非阻塞检查消息
        unsafe {
            match PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE) {
                Ok(_) if msg.message != 0 => {
                    if msg.message == WM_QUIT {
                        info!("收到退出消息，热键监听结束");
                        break;
                    }
                    if msg.message == WM_HOTKEY {
                        // WM_HOTKEY 表示按键按下
                        debug!("WM_HOTKEY 触发");
                        let wp = was_pressed.clone();
                        let pf = press_fired.clone();
                        let op = on_press.clone();
                        if !pf.load(Ordering::SeqCst) {
                            pf.store(true, Ordering::SeqCst);
                            wp.store(true, Ordering::SeqCst);
                            op();
                        }
                    }
                    TranslateMessage(&msg);
                    DispatchMessageW(&msg);
                }
                _ => {}
            }
        }

        // ── 轮询检测按键松开 ──
        let currently_pressed = unsafe {
            let state = GetAsyncKeyState(vk.0 as i32);
            (state & 0x8000u16 as i16) != 0
        };

        let was = was_pressed.load(Ordering::SeqCst);
        let fired = press_fired.load(Ordering::SeqCst);

        if was && !currently_pressed && fired {
            // 按键松开 → 触发 on_release
            was_pressed.store(false, Ordering::SeqCst);
            press_fired.store(false, Ordering::SeqCst);
            debug!("按键松开，触发 on_release");
            on_release();
        }

        // 短暂睡眠避免 CPU 空转
        std::thread::sleep(std::time::Duration::from_millis(POLL_INTERVAL_MS));
    }

    // ── 注销热键 ──
    unsafe {
        let _ = UnregisterHotKey(None, 1);
    }
    info!("热键已注销");

    Ok(())
}
