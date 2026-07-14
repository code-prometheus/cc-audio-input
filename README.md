# audio-input 🎤

**按住鼠标左键 1.5 秒 → 语音命令自动粘贴到 CLI**

> Rust + Sherpa-ONNX + SenseVoice 离线 ASR + LLM 修正 + 系统托盘

---

## 功能

| 步骤 | 说明 |
|------|------|
| ① 按住鼠标左键 1.5 秒 | 开始录音（三声蜂鸣提示） |
| ② 说话 | 对着麦克风说出 CLI 命令（可移动鼠标） |
| ③ 松开鼠标 | 自动停止录音 |
| ④ SenseVoice ASR | 离线语音识别（中/英/日/韩/粤） |
| ⑤ 本地音近词修正 | hotwords.yaml 快速替换常见误识别 |
| ⑥ LLM 二次修正 | 修正 CLI 术语、标点、大小写（最多重试 3 次） |
| ⑦ Ctrl+V 粘贴 | 修正文本自动粘贴到当前窗口（失败标注 `(生文本)`） |
| ⑧ 系统托盘 | 显示最后结果，右键菜单操作 |

### 智能交互

- **拖动检测（阶段一）**：长按触发期间如有拖动操作（>8px），自动取消
- **允许移动鼠标（阶段二）**：录音期间不检测拖动，可自由移动鼠标
- **托盘 tooltip**：鼠标悬停托盘图标时实时显示当前状态
- **等待光标**：录音时系统光标变为沙漏，完成后自动恢复
- **模型缺失提示**：ASR 模型未安装时弹出 Windows 对话框，说明下载地址和安装路径

### 托盘菜单

右键系统托盘图标：
- 🎧 切换麦克风设备（即时生效）
- 📋 拷贝最后结果
- ❌ 退出

---

## 安装

### 1. 解压模型

下载并解压 SenseVoice 模型到 exe 同目录：

```
models/sense-voice-int8/
  ├── model.int8.onnx    (228MB)
  └── tokens.txt
```

下载地址：[sherpa-onnx-sense-voice-int8](https://github.com/k2-fsa/sherpa-onnx/releases/download/asr-models/sherpa-onnx-sense-voice-zh-en-ja-ko-yue-int8-2025-09-09.tar.bz2)

> 如果启动时模型缺失，程序会弹出对话框提示并退出。

### 2. 配置 LLM 模型

编辑 exe 同目录的 `models.yaml`：

```yaml
base_url: "http://your-api:8000/v1"
api_key: "none"
model: "dsv4"
verify_ssl: false
```

支持 OpenAI 兼容 API。

### 3. 确保 sherpa-onnx CLI 可访问

`sherpa-onnx-offline.exe` 和 `onnxruntime.dll` 在 exe 同目录（release 包已自带）。

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

编辑 exe 同目录的 `hotwords.yaml`，支持：

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
git tag v0.7.0 -m "v0.7.0: 阶段二允许移动鼠标 + LLM重试 + 单模型"
git push origin v0.7.0
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
│   ├── main.rs              # 主入口，管道编排
│   ├── config.rs            # 配置加载（models.yaml + 环境变量）
│   ├── trigger.rs           # 鼠标左键长按触发（阶段二允许移动）
│   ├── recorder.rs          # cpal WASAPI + rubato 高质量降采样
│   ├── asr_engine.rs        # Sherpa-ONNX CLI 子进程调用
│   ├── hotwords.rs          # CLI 热词管理 + 本地音近词修正
│   ├── corrector.rs         # LLM 修正器（OpenAI 兼容 API）
│   ├── clipboard_paste.rs   # Win32 剪贴板 + Ctrl+V 模拟
│   ├── device_selector.rs   # 麦克风设备选择
│   └── tray.rs              # 系统托盘 + tooltip + 麦克风切换
├── hotwords.yaml            # 热词配置
├── models.yaml              # LLM 单模型配置
├── assets/                  # 托盘图标等资源
├── sherpa_dll/              # Sherpa-ONNX CLI 可执行文件
├── models/                  # SenseVoice 模型文件（不随 git 分发）
└── Cargo.toml
```

---

## 更新日志

### v0.7.0
- 阶段二允许移动鼠标：录音期间不检测拖动，松手即结束
- LLM 修正最多重试 3 次：结果与生文本一致时立即重试，失败标注 `(生文本)`
- 简化为单模型配置（models.yaml），移除托盘 LLM 切换菜单
- ASR 模型缺失时弹出 Windows 对话框提示下载地址并退出

### v0.6.0
- 音频质量提升：使用 `rubato` 库进行高质量降采样（抗混叠滤波 + 多声道混合）
- 移除 `settings.example.json`，统一使用 `models.yaml` 管理 LLM 配置
- CI 打包流程同步更新

### v0.5.1
- 拖动检测、托盘 tooltip、托盘菜单精简
- 麦克风/LLM 切换即时生效
- ASR SenseVoice JSON 输出正确解析、LLM 无效文本过滤
- 统一 CI/CD 工作流

### v0.5.0
- tray-icon+winit 架构重写
- 鼠标不动检测、等待光标、黑底 A 图标

---

MIT
