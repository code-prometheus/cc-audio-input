# audio-input

按住鼠标左键 1.5 秒 → 录音 → SenseVoice ASR → LLM 修正 → 自动粘贴到 CLI。

## 快速开始

### 1. 下载

从 [Releases](../../releases) 下载 `audio-input-vX.X.X-win-x64.zip`，解压。

### 2. 下载模型

下载 SenseVoice int8 模型 (~117MB) 并解压到 ZIP 目录下的 `models/sense-voice-int8/`：

```
https://github.com/k2-fsa/sherpa-onnx/releases/download/asr-models/
  sherpa-onnx-sense-voice-zh-en-ja-ko-yue-int8-2025-09-09.tar.bz2
```

目录结构：
```
audio-input/
├── audio-input.exe
├── hotwords.yaml
├── settings.example.json
├── sherpa-onnx-offline.exe
├── onnxruntime.dll
├── onnxruntime_providers_shared.dll
└── models/
    └── sense-voice-int8/
        ├── model.int8.onnx
        └── tokens.txt
```

### 3. 配置 LLM

复制 `settings.example.json` → `settings.json`，修改 LLM 地址和密钥：

```json
{
  "llm": {
    "base_url": "http://your-llm:8000/v1",
    "api_key": "sk-your-key",
    "model": "your-model"
  },
  "asr": {
    "model_dir": ""
  }
}
```

不配置 `model_dir` 则自动探测 exe 同级目录或 `F:/models/sense-voice-int8/`。

### 4. 运行

双击 `audio-input.exe`，按住鼠标左键 1.5 秒开始录音，松手自动粘贴。

> **设备选择**: 程序自动匹配 EDIFIER 蓝牙耳机。匹配不到且有多设备时弹出菜单让你选。

## 编译

### 前置要求

- **Rust** 工具链 (通过 [rustup](https://rustup.rs/) 安装): `rustup default stable`
- **MSVC 构建工具**: [Visual Studio Build Tools](https://visualstudio.microsoft.com/downloads/#build-tools-for-visual-studio-2022) 或已安装 VS 2022 (含 "使用 C++ 的桌面开发" 工作负载)
- Windows 10+ x64

### 获取源码

```bash
git clone https://github.com/code-prometheus/cc-audio-input.git
cd cc-audio-input
```

### 编译

```bash
cargo build --release
```

编译产物在 `target/release/audio-input.exe`。

### 准备运行时文件

编译产物需要搭配外部二进制和模型才能运行：

```bash
# sherpa-onnx CLI 工具 + 依赖 DLL (已内置在 assets/ 中, 复制到 exe 同级)
cp assets/sherpa-onnx-offline.exe target/release/
cp assets/onnxruntime.dll target/release/
cp assets/onnxruntime_providers_shared.dll target/release/
cp assets/hotwords.yaml target/release/
cp settings.example.json target/release/settings.json

# 编辑 settings.json 填入你的 LLM 配置
```

模型下载后放到 `models/sense-voice-int8/`（exe 同级目录或 `F:/models/sense-voice-int8/` 均可）。

### 运行开发版

```bash
cargo run --release
```

### 编译优化

Release 配置已启用 `opt-level = "z"` (最小体积)、`lto = true` (链接时优化)、`strip = true` (去除符号)。无需额外操作。

### 依赖清单

| 库 | 用途 |
|---|---|
| `windows` 0.58 | Win32 API (鼠标触发、光标、蜂鸣、托盘) |
| `cpal` 0.15 | WASAPI 音频录制 |
| `reqwest` 0.12 | HTTP 客户端 (LLM API) |
| `serde` + `serde_json` + `serde_yaml` | 配置/热词解析 |
| `log` + `env_logger` | 日志 |
| `anyhow` | 错误处理 |

外部依赖：`sherpa-onnx-offline.exe` + `onnxruntime.dll` (SenseVoice ASR 引擎)。

## License

MIT
