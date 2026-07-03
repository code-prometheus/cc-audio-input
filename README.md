# audio-input

按住鼠标左键说话 → 离线 ASR + LLM 修正 → 自动粘贴到 CLI。

## 快速开始

### 1. 下载

从 [Releases](../../releases) 下载 `audio-input-vX.X.X-win-x64.zip`。

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

## License

MIT
