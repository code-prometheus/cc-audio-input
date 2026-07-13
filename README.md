# audio-input 🎤

**按住鼠标左键 1.5 秒 → 语音命令自动粘贴到 CLI**

> Rust + Sherpa-ONNX + SenseVoice 离线 ASR + LLM 修正 + 系统托盘

---

## 功能

| 步骤 | 说明 |
|------|------|
| ① 按住鼠标左键 1.5 秒 | 开始录音（三声蜂鸣提示） |
| ② 说话 | 对着麦克风说出 CLI 命令 |
| ③ 松开鼠标 | 自动停止录音 |
| ④ SenseVoice ASR | 离线语音识别（中/英/日/韩/粤） |
| ⑤ 本地音近词修正 | hotwords.yaml 快速替换常见误识别 |
| ⑥ LLM 二次修正 | 修正 CLI 术语、标点、大小写 |
| ⑦ Ctrl+V 粘贴 | 修正文本自动粘贴到当前窗口 |
| ⑧ 系统托盘 | 显示最后结果，右键菜单操作 |

### 智能交互

- **拖动检测**：按住鼠标后如有拖动操作（>8px），自动取消录音，不会误触发
- **托盘 tooltip**：鼠标悬停托盘图标时实时显示当前状态（录音中/语音识别中/LLM 修正中）
- **等待光标**：录音时系统光标变为沙漏，完成后自动恢复

### 托盘菜单

右键系统托盘图标：
- 📊 显示最后识别结果
- 🎤 手动开始录音（或双击托盘图标）
- 🎧 切换麦克风设备
- 🤖 切换 LLM 模型
- 📋 拷贝最后结果
- ❌ 退出

---

## 安装

### 1. 解压模型

```bash
# 将 sherpa-onnx-sense-voice-...tar.bz2 解压到 models/ 目录
tar xf sense-voice-model.tar.bz2 -C models/
# 模型目录: models/sense-voice-int8/ (228MB)
```

下载地址：[sherpa-onnx-sense-voice-int8](https://github.com/k2-fsa/sherpa-onnx/releases/download/asr-models/sherpa-onnx-sense-voice-zh-en-ja-ko-yue-int8-2025-09-09.tar.bz2)

### 2. 配置 LLM 模型

编辑 `models.yaml` 配置可用的 LLM 模型列表：

```yaml
active: "默认 (dsv4)"

models:
  - name: "默认 (dsv4)"
    base_url: "http://your-api:8000/v1"
    api_key: "none"
    model: "dsv4"
    verify_ssl: false
```

支持多个模型，通过托盘菜单切换。

### 3. 确保 sherpa-onnx CLI 可访问

`sherpa_dll/sherpa-onnx-v1.13.3-win-x64-shared-MD-Release/bin/sherpa-onnx-offline.exe` 必须在 exe 同目录。

### 4. 运行

```bash
audio-input.exe
```

---

## 环境变量

| 变量 | 默认值 | 说明 |
|------|--------|------|
| `AUDIO_INPUT_HOLD_MS` | 1500 | 鼠标按住触发时长（毫秒） |
| `AUDIO_INPUT_DEVICE_ID` | -1 | 音频设备 ID（-1=默认） |
| `AUDIO_INPUT_MODEL_DIR` | models/sense-voice-int8 | 模型路径 |

---

## 热词自定义

编辑项目同目录的 `hotwords.yaml`，支持：

- `claude_code_commands` — Claude Code 命令列表
- `cli_tools` — CLI 工具名称
- `common_options` — 通用命令选项
- `phonetic_corrections` — 音近词修正映射（ASR误识 → 正确词）
- `filler_words` — 口语填充词列表

---

## 开发

### 编译

```bash
cargo build --release
```

### 测试

```bash
# 虚拟语音测试（模拟 ASR 错误 → LLM 修正）
python test_asr_virtual.py
```

### 发布

推送 tag 会触发 GitHub Actions 自动编译打包发布：

```bash
git tag v0.5.1 -m "v0.5.1: 拖动检测 + tooltip + 文档更新"
git push origin v0.5.1
```

---

## 技术栈

- **Rust** — 编译为单文件 exe
- **Sherpa-ONNX CLI** — SenseVoice 推理引擎
- **SenseVoice (阿里)** — 离线多语言 ASR（中/英/日/韩/粤）
- **OpenAI 兼容 API** — LLM 文本修正
- **Win32 API** — 鼠标监听 / 剪贴板 / 键盘模拟 / 系统托盘

---

## 项目结构

```
audio-input/
├── src/
│   ├── main.rs            # 主入口，管道编排
│   ├── config.rs          # 配置加载（models.yaml + 环境变量）
│   ├── trigger.rs         # 鼠标左键长按 + 拖动检测 + 托盘手动触发
│   ├── recorder.rs        # cpal WASAPI 录音（16kHz mono f32）
│   ├── asr_engine.rs      # Sherpa-ONNX CLI 子进程调用
│   ├── hotwords.rs        # CLI 热词管理 + 本地音近词修正
│   ├── corrector.rs       # LLM 修正器（OpenAI 兼容 API）
│   ├── clipboard_paste.rs # Win32 剪贴板 + Ctrl+V 模拟
│   ├── device_selector.rs # 麦克风设备选择
│   └── tray.rs            # 系统托盘 + tooltip + 右键菜单
├── hotwords.yaml          # 热词配置
├── models.yaml            # LLM 模型配置
├── assets/                # 托盘图标等资源
├── sherpa_dll/            # Sherpa-ONNX CLI 可执行文件
├── models/                # SenseVoice 模型文件（不随 git 分发）
└── Cargo.toml
```

---

## 更新日志

### v0.5.1
- 拖动检测：按住左键后拖动鼠标自动取消录音
- 托盘 tooltip：实时显示录音/识别/修正状态
- 托盘菜单显示最后识别结果
- 统一 CI/CD 工作流

### v0.5.0
- tray-icon+winit 架构重写
- 鼠标不动检测、等待光标
- 黑底 a 图标
- LLM 修正优化

### v0.4.0
- 托盘菜单增强
- LLM 多模型支持
- 热词优化
- CI/CD 配置

---

MIT
