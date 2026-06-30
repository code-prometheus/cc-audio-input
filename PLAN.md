# 语音+LLM CLI输入工具 — 项目计划 v2

> 项目代号: audio-input
> 目标平台: Windows 10/11 x64
> 语言/框架: Rust + Sherpa-ONNX + SenseVoice
> 更新日期: 2026-06-30

---

## 一、核心需求变更 (v2 vs v1)

| 需求 | v1 | v2 |
|------|----|-----|
| 触发方式 | F2 热键按下/松开 | **按住鼠标左键3秒** 触发录音，松开完成识别 |
| 托盘图标 | 可选 Phase 4 | **必须实现**，显示最后识别结果，可点击拷贝 |
| LLM后端 | Claude/OpenAI/Ollama 可选 | **仅 OpenAI 兼容接口**，配置在 `settings.json` |
| 发行方式 | 仅编译 | **发行版可执行文件**，带模型捆绑方案 |
| 调试模式 | 手动 | **完全自动**：自动运行、自动修复编译错误 |

---

## 二、技术栈确定

| 层面 | 选型 | 版本/说明 |
|------|------|-----------|
| 编程语言 | Rust | 1.96.0，stable-x86_64-pc-windows-gnu |
| ASR推理框架 | Sherpa-ONNX (FFI) | 加载 sherpa_onnx.dll |
| ASR模型 | SenseVoice int8 | 228MB，中/英/日/韩/粤 |
| 音频采集 | cpal (WASAPI) | Phase 1用，Phase 2切Sherpa内置 |
| 热键触发 | Win32 GetAsyncKeyState | 轮询鼠标左键(VK_LBUTTON)状态 |
| LLM修正 | OpenAI兼容API | 从 settings.json 读取 |
| 剪贴板 | Win32 Clipboard API | CF_UNICODETEXT |
| CLI粘贴 | Win32 keybd_event | 模拟 Ctrl+V |
| 系统托盘 | tray-icon crate | Windows原生托盘 |
| 配置 | settings.json (JSON) | 仅LLM配置 |
| 日志 | env_logger | 结构化日志 |

### 项目目录结构 (v2)
```
F:\audio_input\
├── Cargo.toml
├── settings.json               # 用户LLM配置（OpenAI兼容接口）
├── config.yaml                 # 弃用，保留兼容
├── assets/
│   └── hotwords.yaml            # CLI热词库
├── models/                      # SenseVoice模型（自动下载脚本）
│   └── sense-voice-int8/
│       ├── model.int8.onnx
│       └── tokens.txt
├── src/
│   ├── main.rs                  # 主入口 + 托盘
│   ├── config.rs                # 配置（settings.json）
│   ├── trigger.rs               # 鼠标左键3秒长按检测（新）
│   ├── recorder.rs              # 音频采集
│   ├── asr_engine.rs            # ASR引擎
│   ├── hotwords.rs              # 热词管理
│   ├── corrector.rs             # LLM修正（仅OpenAI兼容）
│   ├── clipboard_paste.rs      # 剪贴板+粘贴
│   └── tray.rs                  # 系统托盘管理（新）
├── build.bat                    # 构建脚本
├── README.md                    # 使用文档
├── RESEARCH.md
└── PLAN.md                      # 本文件
```

---

## 三、模块设计

### 3.0 触发模块 (`trigger.rs`) - NEW
**职责**: 检测鼠标左键长按3秒

**实现方案**:
- 独立线程轮询 `GetAsyncKeyState(VK_LBUTTON)`
- 按下时记录时间戳
- 按住超过3秒 → 触发 on_trigger（开始录音）
- 松开 → 触发 on_release（停止录音→识别→修正→粘贴）
- 短按（<3秒）忽略，作为普通鼠标点击处理

### 3.1 系统托盘 (`tray.rs`) - NEW
**职责**: Windows状态栏托盘图标 + 气泡通知

**实现方案**:
- 使用 `tray-icon` crate
- 托盘图标显示程序状态（空闲/录音/识别中）
- 右键菜单: 退出、显示设置、拷贝最后结果
- 气泡通知显示最后一条识别结果
- 点击气泡/双击托盘可拷贝最后结果到剪贴板

### 3.2-3.7 其余模块保持不变（config/clipboard/hotwords/corrector/recorder/asr）

---

## 四、settings.json 规范
```json
{
  "llm": {
    "base_url": "https://api.openai.com/v1",
    "api_key": "sk-xxx",
    "model": "gpt-4o-mini"
  },
  "hotkey": {
    "hold_ms": 3000
  }
}
```

---

## 五、分阶段计划 (v2)

### Phase 0: 环境准备
- Rust 环境验证
- 依赖下载（cargo fetch）

### Phase 1: 核心管线（占位ASR）
- settings.json 加载
- 鼠标左键3秒检测
- cpal 录音
- ASR占位
- OpenAI兼容LLM修正
- 剪贴板+粘贴
- 主流程串联

### Phase 2: ASR集成
- 下载Sherpa-ONNX DLL
- 下载SenseVoice模型
- FFI绑定Sherpa C API
- 真实ASR替换占位

### Phase 3: 系统托盘 + 发行
- 托盘图标+菜单
- 气泡通知
- 编译Release版本
- 测试自动运行

### Phase 4: 自动化调试
- 编译错误自动修复
- 运行测试
- 最终发行包制作

---

## 六、构建和发行
```bash
# Release构建
cargo build --release
# 输出: target/release/audio-input.exe

# 发行包结构
audio-input-v1.0.0/
├── audio-input.exe
├── settings.json
├── hotwords.yaml
├── models/sense-voice-int8/  (需预下载)
├── sherpa_onnx.dll
├── onnxruntime.dll
└── README.md
```
