//! 系统托盘模块 — Win32 原生实现
//! 托盘图标 + tooltip + 气泡通知 + 右键拷贝

use anyhow::Result;
use log::info;
use std::sync::{Arc, Mutex};

pub struct TrayManager {
    last_result: Arc<Mutex<String>>,
}

impl TrayManager {
    /// 创建托盘（Win32 原生方式）
    pub fn create(tooltip: &str) -> Result<(Self, Arc<Mutex<String>>)> {
        let last_result = Arc::new(Mutex::new(String::new()));
        info!("📌 托盘就绪 (tooltip: {})", tooltip);
        // Phase 3 full impl: 使用 Shell_NotifyIconW + CreateWindow 创建托盘
        // 当前返回 stub
        Ok((Self { last_result: last_result.clone() }, last_result))
    }

    /// Stub without actual tray
    pub fn stub() -> Self {
        Self { last_result: Arc::new(Mutex::new(String::new())) }
    }

    pub fn update_result(&self, text: &str) {
        if let Ok(mut r) = self.last_result.lock() {
            *r = text.to_string();
        }
    }

    pub fn show_notification(&self, title: &str, body: &str) {
        info!("💬 {}: {}", title, body);
    }
}
