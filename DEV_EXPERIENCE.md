# audio-input 项目编程经验文档

> 面向 Claude Code 的经验总结，帮助将来类似项目快速解决同类问题。

---

## 1. winit 线程模型挑战

### 问题

Windows GUI 框架（winit/tray-icon）通常要求 EventLoop 在主线程运行。但 audio-input 的主线程被 `trigger::listen()` 的死循环占用（50ms 轮询鼠标状态），无法同时运行 EventLoop。

### 尝试过的方案

1. **Shell_NotifyIconW + 隐藏窗口 + GetMessageW**（v0.4.0）：用 Win32 原生 API 而不是 winit。工作正常但代码冗余，菜单管理不灵活。

2. **Trigger 移到子线程**：让主线程跑 winit，trigger 循环在子线程。但 Windows `SetSystemCursor`/`SetCursor` 等 API 在主线程表现更好。

### 最终方案（v0.5.0+）

用 winit 的 `EventLoopBuilderExtWindows::with_any_thread(true)` 让 EventLoop 在子线程运行：

```rust
use winit::platform::windows::EventLoopBuilderExtWindows;

let event_loop = winit::event_loop::EventLoop::builder()
    .with_any_thread(true)  // 关键！允许非主线程
    .build()?;
```

托盘线程完全独立，主线程继续跑 trigger 循环。两者通过 `mpsc::channel` 通信。

**教训：** Rust Windows GUI 生态不如 C#/WinForms 成熟，遇到线程限制先查 winit 的 Windows 扩展 API。

---

## 2. 鼠标拖动检测

### 问题

用户按住鼠标左键可能有两个意图：
- **长按不动** → 想语音输入（触发 ASR）
- **拖动选中文字** → 只是想选中内容（不应触发）

v0.5.0 只检测按住时长，不检测位移。拖动文字也被触发录音。

### 方案

用 `GetCursorPos` API 在按下瞬间记录锚点，每次轮询检查偏移。超过阈值 → 取消。

```rust
use windows::Win32::Foundation::POINT;
use windows::Win32::UI::WindowsAndMessaging::GetCursorPos;

const DRAG_THRESHOLD: i32 = 8; // 像素

// 按下时记录位置
let anchor_x; let anchor_y;
unsafe {
    let mut p = POINT::default();
    let _ = GetCursorPos(&mut p);
    anchor_x = p.x; anchor_y = p.y;
}

// 轮询中检测
unsafe {
    let mut p = POINT::default();
    let _ = GetCursorPos(&mut p);
    if (p.x - anchor_x).abs() > DRAG_THRESHOLD
        || (p.y - anchor_y).abs() > DRAG_THRESHOLD {
        // 拖动 → 取消
        return false;
    }
}
```

### 阈值选择

- 8px：足够区分"手指轻微抖动"（1-3px）和"有意拖动"（通常 > 10px）
- 可根据需要调整，太小 → 误取消，太大 → 误触发

### 两个检测点

需要在**两个阶段**都检测：
1. `wait_for_trigger()` 内 — 按住等待期 → 返回 false
2. `listen()` 的松开等待期 — 录音已开始，需 `on_cancel()` 回调恢复状态

---

## 3. tray-icon tooltip 跨线程通信

### 架构

```
main.rs                    tray 线程 (winit EventLoop)
   │                            │
   ├─ tray::set_tooltip("🔴")   │
   │  └→ mpsc::Sender           │
   │       └→ channel ─────────→ mpsc::Receiver
   │                            │  └→ about_to_wait() 中 try_recv()
   │                            │     └→ tray.set_tooltip(Some(tt))
```

### 注意点

- `set_tooltip()` 必须在 `run_tray_in_thread()` **之后**调用，否则 `G_TOOLTIP_TX` 还是 `None`，调用无效
- `about_to_wait()` 用 `ControlFlow::Poll` 确保高频轮询 channel
- 不要用 `crossbeam_channel` 的 `Receiver`（它用于 tray menu events），tooltip 用独立的 `mpsc::channel`

### 为什么不用 balloon notification

`tray-icon` crate (v0.19) 只提供 `set_tooltip()`，不暴露 `Shell_NotifyIcon` 的 `NIIF_INFO` balloon 功能。如果需要 balloon，要么自己封装 Win32 API，要么换库。目前的 tooltip 方式更轻量，hover 托盘图标时显示即可。

---

## 4. sherpa-onnx FFI vs CLI 子进程

### 问题

sherpa-onnx 提供 C FFI（`sherpa-onnx.dll` + Rust bindings），但遇到结构体对齐问题：
- C 端 `SherpaOnnxOfflineRecognizerConfig` 包含嵌套结构体和联合体
- Rust 端 `#[repr(C)]` 手动对齐容易出错
- 字段映射错了 → 运行时崩溃或静默错误结果

### 方案

用 `sherpa-onnx-offline.exe` CLI 子进程：
- 输入：wav 文件（临时写入 + stdin 管道）
- 输出：stdout 文本
- 无 FFI 复杂度，进程隔离

代价：每次识别需启动子进程 + 写临时文件（~100ms 额外开销）。

### 教训

对于不熟悉的 C FFI 库，CLI 子进程方式更可靠。性能差异在语音识别场景下可接受（ASR 本身 > 1s，100ms 开销占比小）。

---

## 5. CI/CD 工作流陷阱

### 问题

项目有**两个** release workflow：
- `ci.yml` 的 `release` job（tag 触发，依赖 build-and-test）
- `release.yml`（独立，同样 tag 触发）

两者同时触发创建重复的 GitHub Release，且打包逻辑不同：
- ci.yml 从 `sherpa_dll/` 复制文件
- release.yml 从 `assets/` 复制文件

### 解决

删除 `release.yml`，统一在 `ci.yml` 中：
- build-and-test job 编译 + 打包 ZIP + 上传 artifact
- release job 从 artifact 下载 + `gh release create`

ZIP 内容：exe + hotwords.yaml + models.yaml + settings.example.json + sherpa_dll 二进制 + INSTALL.txt

---

## 6. Rust Release 体积优化

```toml
[profile.release]
opt-level = "z"       # 优化体积（不是速度）
lto = true            # 链接时优化
strip = true          # 去除调试符号
codegen-units = 1     # 单一编译单元，更激进优化
```

效果：audio-input.exe 从 ~8MB 降到 ~3.5MB。

---

## 7. 代理设置

开发环境访问外部 API（GitHub、npm、pip、cargo 等）通过 `http://localhost:60130` 代理，不做 SSL 证书检查：

```bash
# Git
git config --global http.proxy http://localhost:60130
git config --global https.proxy http://localhost:60130

# Cargo
# 在 .cargo/config.toml 中：
[http]
proxy = "http://localhost:60130"

[net]
git-fetch-with-cli = true
```

---

## 8. Cargo.toml 的 `windows_subsystem = "windows"` 坑

```rust
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
```

在 release 模式下不显示控制台窗口。但 debug 模式下显示（方便看 log）。

**坑**: release 模式下 `println!` 和 `eprintln!` 无输出（没有控制台）。必须用日志系统输出到文件。

---

## 总结：audio-input 项目中的关键决策

| 决策 | 方案 | 理由 |
|------|------|------|
| GUI 框架 | tray-icon + winit (子线程) | `with_any_thread(true)` 解决线程冲突 |
| ASR 引擎 | CLI 子进程 | 避免 FFI 结构体对齐 |
| HTTP 客户端 | reqwest blocking | 简单，无需 async runtime |
| 跨线程通信 | mpsc::channel | 轻量，适合简单消息 |
| 拖动检测 | GetCursorPos + 8px 阈值 | 区分"按住"和"拖动" |
| 状态提示 | 托盘 tooltip | tray-icon 原生支持，无需 balloon |
| 体积优化 | opt=z + lto + strip | 单文件 exe 从 8MB 降到 3.5MB |
| CI/CD | 单 workflow (ci.yml) | 避免重复 Release 冲突 |
