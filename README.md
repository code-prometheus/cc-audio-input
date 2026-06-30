# audio-input 🎤

**按住鼠标左键 3 秒 → 语音命令自动粘贴到 CLI**

> Rust + Sherpa-ONNX + SenseVoice (阿里通义千问) 离线 ASR + LLM 修正

---

## 功能

| 步骤 | 说明 |
|------|------|
| 1. 按住鼠标左键 3 秒 | 开始录音 |
| 2. 说话 | 对着麦克风说出 CLI 命令 |
| 3. 松开鼠标 | 自动停止录音 |
| 4. SenseVoice ASR | 离线语音识别 (中/英/日/韩/粤) |
| 5. LLM 修正 | 修正 CLI 术语拼写和语法 |
| 6. Ctrl+V 粘贴 | 修正文本自动粘贴到 CLI 窗口 |
| 7. 系统托盘 | 显示最后识别结果，右键可拷贝 |

## 安装

### 1. 下载模型

```batch
download_model.bat
```

模型安装到: `F:\models\sense-voice-int8\` (228MB)

### 2. 配置 LLM

编辑 `src/config.rs` 中的 LLM 配置:

```rust
llm: LlmConfig {
    base_url: "http://122.1.231.24:8000/v1",
    api_key: "none",
    model: "dsv4",
    verify_ssl: false,
}
```

### 3. 运行

```batch
audio-input.exe
```

## 环境变量

| 变量 | 默认值 | 说明 |
|------|--------|------|
| `AUDIO_INPUT_HOLD_MS` | 3000 | 鼠标按住触发时长 |
| `AUDIO_INPUT_DEVICE_ID` | -1 | 音频设备ID (-1=默认) |
| `AUDIO_INPUT_MODEL_DIR` | F:/models/sense-voice-int8 | 模型路径 |

## 开发

```bash
# 编译
cargo build --release

# 发布打包
build_release.bat
```

## 技术栈

- **Rust** — 编译为单文件 exe
- **Sherpa-ONNX C API** — SenseVoice 推理引擎
- **SenseVoice** — 阿里通义千问离线中文 ASR (中/英/日/韩/粤)
- **OpenAI 兼容 API** — LLM 文本修正
- **Win32 API** — 鼠标监听/剪贴板/键盘模拟

## 项目结构

```
src/
  main.rs              # 主入口
  config.rs             # 硬编码配置
  trigger.rs            # 鼠标左键长按检测
  recorder.rs           # cpal WASAPI 录音
  asr_engine.rs         # Sherpa-ONNX FFI 绑定
  hotwords.rs           # CLI 热词管理
  corrector.rs          # LLM 修正器
  clipboard_paste.rs   # Win32 剪贴板 + Ctrl+V
  device_selector.rs   # 麦克风设备选择
  tray.rs               # 系统托盘
```

## License

MIT
