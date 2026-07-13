# CLAUDE.md — audio-input 项目指引

## 项目概述

audio-input 是一个 Windows 语音输入工具：按住鼠标左键 1.5 秒触发录音 → SenseVoice ASR 离线识别 → LLM 修正 → Ctrl+V 粘贴到当前窗口。

## 技术架构

```
鼠标监听 (trigger.rs) ─→ cpal 录音 (recorder.rs) ─→ sherpa-onnx CLI 子进程 (asr_engine.rs)
                                                              │
                                                              ▼
                                    Ctrl+V 粘贴 ← LLM 修正 (corrector.rs) ← 热词本地修正 (hotwords.rs)
                                  (clipboard_paste.rs)
```

### 线程模型

- **主线程**: `trigger::listen()` 鼠标/托盘触发轮询循环
- **录音线程**: 每次触发时 spawn，cpal 录音到缓冲区
- **托盘线程**: tray-icon + winit EventLoop (with_any_thread=true)
- **ASR 调用**: 阻塞式，在 on_release 回调中同步执行
- **LLM 调用**: 阻塞式 HTTP (reqwest blocking client)，15 秒超时

### 关键设计决策

1. **托盘用 tray-icon + winit 子线程**: 因为主线程被 trigger 循环占用，用 `with_any_thread(true)` 在子线程跑 EventLoop
2. **ASR 用子进程 CLI**: 避免 sherpa-onnx FFI 结构体对齐问题
3. **LLM 用 reqwest blocking**: 简单直接，无需 async runtime
4. **拖动检测**: `GetCursorPos` 记录按下时位置，hold 期间偏移 > 8px 视为拖动，取消录音
5. **Tooltip 跨线程通信**: `tray::set_tooltip()` 通过 `mpsc::channel` 发送到托盘线程，`about_to_wait()` 中 poll 更新

### 触发流程（v0.5.1+）

```
鼠标左键按下 → 记录位置 + 计时
  ├─ 短按 (< hold_ms) → 忽略
  ├─ 拖动 (> 8px) → 取消 (返回 false)
  └─ 长按 (≥ hold_ms) → on_trigger() → 录音
       ├─ 松开鼠标 → on_release() → ASR → LLM → paste
       └─ 拖动鼠标 → on_cancel() → 恢复状态, 不执行 ASR/LLM
```

### Tooltip 状态机

```
"audio-input 🎤" (空闲)
  → "🔴 录音中..." (on_trigger)
  → "📝 语音识别中..." (on_release 开始)
  → "🤖 LLM 修正中..." (ASR 完成后)
  → "audio-input 🎤" (LLM 完成后)
```

## 构建

```bash
cargo build --release
```

### 前置条件

- `sherpa_dll/sherpa-onnx-v1.13.3-win-x64-shared-MD-Release/bin/sherpa-onnx-offline.exe` 存在
- `models/sense-voice-int8/model.int8.onnx` + `tokens.txt` 存在
- `hotwords.yaml` 在 exe 同目录
- `models.yaml` 在 exe 同目录

### Release 配置

`Cargo.toml` 中的 release profile:
- `opt-level = "z"` — 优化体积
- `lto = true` — 链接时优化
- `strip = true` — 去除调试符号
- `codegen-units = 1` — 更激进的优化

## 配置文件

| 文件 | 格式 | 用途 |
|------|------|------|
| `hotwords.yaml` | YAML | CLI 术语表、音近词修正、填充词 |
| `models.yaml` | YAML | LLM 模型列表，可通过托盘菜单切换 |

## 测试

```bash
# 虚拟语音测试 — 模拟 ASR 错误 → LLM 修正管道
python test_asr_virtual.py

# 编译检查
cargo check
```

## 添加新功能注意事项

- **修改 `trigger.rs` 时**: `listen()` 签名有 5 个参数 (hold_ms, trigger_rx, on_trigger, on_release, on_cancel)，三个回调都需要 `Send + 'static`
- **修改 `tray.rs` 时**: 菜单 ID 统一用 `__xxx__` 格式（如 `__record__`, `__exit__`）；`menu_ids` 模块已移除，直接用字符串
- **修改 `hotwords.yaml` 后**: 无需重新编译，exe 启动时加载
- **修改 `models.yaml` 后**: 需要重启程序才能生效
- **音近词修正规则**: key 用**全小写**，`quick_correct` 做大小写不敏感匹配
- **Tooltip 调用 `tray::set_tooltip()`**: 在 `G_TOOLTIP_TX` 初始化后（`run_tray_in_thread()` 调用后）才可用
- **`G_LAST_RESULT` 写入**: 通过 `tray::set_last_result(&text)` 写入，托盘菜单 status 项自动读取

## 代理设置

开发环境访问外部 API 通过 `http://localhost:60130` 代理。
