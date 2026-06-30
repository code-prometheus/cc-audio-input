# audio-input

**Windows 语音转 CLI 输入工具** -- 按住热键说话，松开后语音自动转为文本并粘贴到当前 CLI 窗口。

使用 **SenseVoice (阿里)** 离线 ASR 引擎，通过 **Sherpa-ONNX** 推理，支持 LLM 修正识别结果中的 CLI 专有术语。

---

## 功能

- **按住热键说话，松开即粘贴** -- PTT (Push-to-Talk) 工作流
- **离线语音识别** -- SenseVoice ONNX 模型，无需网络
- **LLM 修正** -- Claude / OpenAI / Ollama 修正 CLI 术语拼写错误
- **CLI 热词增强** -- 预置 Claude Code / git / npm / docker 等常见命令词库
- **零运行时依赖** -- Go 编译为单个 .exe
- **支持 Windows 10/11 x64**

---

## 快速开始

### 1. 下载

```bash
git clone <repo-url> && cd audio-input
```

### 2. 安装依赖

- **Go >= 1.21** (需要 CGO 支持)
- **Sherpa-ONNX Windows DLL** (可从 [Sherpa Releases](https://github.com/k2-fsa/sherpa-onnx/releases) 下载)
- **SenseVoice 模型** (~228MB)

```bash
# 下载 SenseVoice int8 模型
make download-model
```

### 3. 配置

复制 `configs/config.yaml` 到可执行文件目录，修改：

```yaml
asr:
  model_path: "./models/sense-voice-int8/model.int8.onnx"
  tokens_path: "./models/sense-voice-int8/tokens.txt"

corrector:
  enabled: true
  backend: "claude"
  api_key: "${ANTHROPIC_API_KEY}"  # 或通过环境变量 AUDIO_INPUT_CORRECTOR_API_KEY
```

### 4. 构建并运行

```bash
make build
./build/audio-input.exe
```

按下 `F2` 开始录音，松开 `F2` 自动识别并粘贴。

### 配置环境变量

所有配置项均可通过环境变量覆盖（前缀 `AUDIO_INPUT_`）：

```bash
set AUDIO_INPUT_ASR_MODEL_PATH=C:\models\sense-voice-int8\model.int8.onnx
set AUDIO_INPUT_ASR_TOKENS_PATH=C:\models\sense-voice-int8\tokens.txt
set AUDIO_INPUT_CORRECTOR_API_KEY=sk-ant-...
set AUDIO_INPUT_HOTKEY_VK_CODE=0x71  # F2
```

---

## 架构

```
热键按下(F2) 
  → WASAPI 麦克风录音 (16kHz mono)
  → SenseVoice ONNX 离线 ASR 识别 
  → [LLM 修正: CLI 术语拼写规范化] 
  → 剪贴板写入 + Ctrl+V 粘贴
```

### 目录结构

```
audio-input/
├── cmd/audio_input/main.go       # 入口
├── internal/
│   ├── hotkey/hotkey.go           # 全局热键 (Win32 RegisterHotKey)
│   ├── audio/recorder.go          # 麦克风录音 (WASAPI via Sherpa)
│   ├── asr/recognizer.go          # ASR 识别 (SenseVoice via Sherpa-ONNX)
│   ├── corrector/llm.go           # LLM 修正 (Claude/OpenAI/Ollama)
│   ├── clipboard/paste.go         # 剪贴板 + Ctrl+V 粘贴
│   ├── hotwords/hotwords.go       # CLI 热词管理
│   └── config/config.go           # YAML 配置加载
├── configs/config.yaml            # 默认配置模板
├── assets/hotwords.yaml           # CLI 专有词库
└── Makefile
```

---

## 热键

默认热键：**F2** (可配置)

修改方式：编辑 `config.yaml` 中 `hotkey.vk_code` 或设置 `AUDIO_INPUT_HOTKEY_VK_CODE`。

常用虚拟键码：
| 键 | VK Code |
|----|---------|
| F1 | 0x70 |
| F2 | 0x71 |
| F12 | 0x7B |

---

## LLM 后端

支持三种后端：

| 后端 | 配置值 | API Key 来源 | 说明 |
|------|--------|-------------|------|
| Claude API | `claude` | `AUDIO_INPUT_CORRECTOR_API_KEY` | 推荐 (用户已在 Claude Code 中) |
| OpenAI API | `openai` | `AUDIO_INPUT_CORRECTOR_API_KEY` | 备选 |
| 本地 Ollama | `ollama` | 不需要 | 完全离线，需先运行 `ollama serve` |

---

## 热词

CLI 热词表位于 `assets/hotwords.yaml`，包含：

- Claude Code 命令 (`/help`, `/clear`, `/compact` 等)
- CLI 工具 (git, npm, pip, docker, cargo 等)
- 常用选项 (--verbose, --debug, --force 等)
- 音近词修正映射 (e.g., "close" → "claude", "吉特" → "git")

在识别结果中出现的这些术语会被优先修正。

---

## 性能

| 指标 | 数值 |
|------|------|
| ASR 模型 | SenseVoice int8 (228MB) |
| 识别延迟 | < 1s (正常语速一句话) |
| LLM 修正延迟 | < 2s (Claude API) 或 < 0.5s (本地 Ollama) |
| 端到端延迟 | < 3s |

---

## 技术栈

- **语言**: Go 1.21+
- **ASR**: SenseVoice (阿里通义千问) on Sherpa-ONNX (k2-fsa)
- **LLM**: Claude API / OpenAI API / Ollama
- **音频**: WASAPI (Windows Audio Session API)
- **热键**: Win32 RegisterHotKey + GetAsyncKeyState

---

## License

MIT
