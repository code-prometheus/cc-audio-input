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
- **托盘线程**: Win32 原生 API (Shell_NotifyIconW + 隐藏窗口 + GetMessageW)
- **ASR 调用**: 阻塞式，在 on_release 回调中同步执行
- **LLM 调用**: 阻塞式 HTTP (reqwest blocking client)，15 秒超时

### 关键设计决策

1. **托盘不是 tray-icon+winit**: 因为主线程被 trigger 循环占用，无法跑 winit EventLoop
2. **托盘用 Win32 原生 API**: `Shell_NotifyIconW` + 隐藏窗口 + 手动消息循环
3. **ASR 用子进程 CLI**: 避免 sherpa-onnx FFI 结构体对齐问题
4. **LLM 用 reqwest blocking**: 简单直接，无需 async runtime

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

- 修改 `tray.rs` 时：菜单 ID 必须在 `menu_ids` 模块中定义，WM_COMMAND 处理在 `handle_menu_action` 中
- 修改 `hotwords.yaml` 后：无需重新编译，exe 启动时加载
- 修改 `models.yaml` 后：需要重启程序才能生效
- 音近词修正规则：key 用**全小写**，`quick_correct` 做大小写不敏感匹配

## 代理设置

开发环境访问外部 API 通过 `http://localhost:60130` 代理。
