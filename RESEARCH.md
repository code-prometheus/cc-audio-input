# 语音+LLM CLI输入工具 — 技术调研报告

> 调研日期: 2026-06-29
> 目标: 在 Windows 主机上实现「按住热键→语音→ASR→LLM修正→粘贴到CLI」全流程

---

## 一、LLM网页版ASR方案调研

各大主流LLM产品的语音输入实现技术总结：

| LLM产品 | ASR引擎 | 模式 | 中文支持 | API可用性 |
|---------|---------|------|----------|-----------|
| **ChatGPT** (OpenAI) | OpenAI Whisper API | 在线 | 良好 | ✅ `/v1/audio/transcriptions`，收费 |
| **Claude.ai** (Anthropic) | 未公开（推测Whisper变体/第三方） | 在线 | 良好 | ❌ 无公开ASR API |
| **Gemini** (Google) | Google Chirp (USM系列) | 在线 | 良好 | ✅ Google Cloud STT API |
| **DeepSeek** | 自研，未公开 | 在线 | 良好 | ❌ 无公开API |
| **Kimi** (月之暗面) | 未公开 | 在线 | 良好 | ❌ 无公开API |
| **通义千问** (阿里) | **FunASR / Paraformer / SenseVoice** | 在线+**开源离线** | **最优** | ✅ SenseVoice开源 |
| **文心一言** (百度) | 百度 DeepSpeech | 在线 | 良好 | ✅ 百度云STT API（收费） |

### 关键发现

1. **绝大多数LLM厂商使用在线ASR**，不开放API给第三方
2. **阿里是唯一同时开源ASR模型（SenseVoice/FunASR）的LLM厂商**
3. OpenAI Whisper是事实上的工业标准，API可用但收费
4. 没有一个厂商的ASR方案适合我们这种"本地+离线"场景

---

## 二、开源中文ASR方案详细对比

### 2.1 核心候选方案

| 维度 | SenseVoice (阿里) | Whisper large-v3 (OpenAI) | FunASR Paraformer (阿里) | Sherpa Zipformer (k2-fsa) |
|------|-------------------|--------------------------|--------------------------|---------------------------|
| **中文准确率** | ~2.5% CER (最优) | ~8-10% WER (中文) | ~2.8% CER | ~4-5% WER |
| **多语言** | 中/英/日/韩/粤5种 | 99种语言 | 中文+英文 | 中/英/日/韩等多模型 |
| **原生流式** | 模拟流式（分段） | ❌ 不支持 | ✅ 在线流式 | ✅ 原生流式 |
| **离线运行** | ✅ | ✅ | ✅ | ✅ |
| **Windows支持** | ✅ (ONNX Runtime) | ✅ (whisper.cpp) | ✅ (ONNX) | ✅ (ONNX Runtime) |
| **模型体积** | **~228MB (int8)** | ~3GB (fp16) | ~220MB | 几十MB |
| **推理框架** | Sherpa-ONNX (ONNX RT) | whisper.cpp (GGML) | FunASR (PyTorch/ONNX) | Sherpa-ONNX (ONNX RT) |
| **VAD支持** | ✅ (Silero VAD) | ✅ (需额外集成) | ✅ | ✅ |
| **标点恢复** | ✅ | ✅ (部分) | ✅ | ✅ |
| **热词增强** | ✅ (transducer模型) | ❌ | ✅ | ✅ (transducer模型) |
| **编程语言绑定** | Go/Rust/C++/C/Python/C#/Java/JS/Swift/Dart/Kotlin/Pascal | Python/C++ | Python/C++ | Go/Rust/C++/C/Python/C#/Java/JS/Swift/Dart/Kotlin/Pascal |
| **社区活跃度** | ⭐⭐⭐⭐⭐ (阿里维护+Sherpa社区) | ⭐⭐⭐⭐⭐ (OpenAI) | ⭐⭐⭐⭐ (阿里达摩院) | ⭐⭐⭐⭐ (k2-fsa社区) |
| **GitHub Stars** | 集成在Sherpa中 | 70k+ (whisper.cpp) | 5k+ (FunASR) | 3k+ (sherpa-onnx) |

### 2.2 其他方案快速评估

| 方案 | 结论 |
|------|------|
| **PaddleSpeech** | 百度开源，中文WER约6-8%，Windows部署困难（深度依赖Linux生态），不推荐 |
| **Coqui STT** | 项目已停止维护（2024年归档），不推荐 |
| **DeepSpeech** (Mozilla) | 已停止维护，不推荐 |
| **Vosk** | 轻量离线，中文模型质量一般（~15% WER），不推荐 |
| **WeNet** | 学术项目，中文尚可但Windows支持薄弱，备选 |

---

## 三、核心决策：离线ASR vs 在线ASR

### 结论：**强烈推荐离线ASR**

理由：
1. **零延迟**：无网络往返，识别速度更快
2. **零成本**：无API调用费用，无次数限制
3. **隐私安全**：语音数据不出本地，适合敏感命令
4. **高可用**：无网络也能使用
5. **可控性**：热词增强（Hotwords）在本地直接完成，不需要额外加密/解密
6. **模型质量已经足够**：SenseVoice 中文 ~2.5% CER，对于CLI命令场景绰绰有余

### 关于"在线ASR做备选"

可考虑保留一个可选的在线ASR后端（如Whisper API），当用户说含复杂专有名词的句子、离线ASR识别文本疑似有问题时使用。但这不是必须项，Phase 1 不做。

---

## 四、ASR引擎最终推荐：SenseVoice on Sherpa-ONNX

**推荐：SenseVoice (阿里通义千问团队) + Sherpa-ONNX (k2-fsa) 推理框架**

理由详细分解：

1. **中文最佳**：SenseVoice是当前开源中文ASR的SOTA，~2.5% CER（字符错误率），远超Whisper
2. **模型轻量**：int8量化仅228MB，适合桌面端部署
3. **Sherpa-ONNX是完美的推理框架**：
   - 完全离线，无需任何网络连接
   - Windows x64 预编译二进制/库文件可直接下载
   - **Go语言官方绑定**（支持Windows x64），提供`real-time-speech-recognition-from-microphone`完整示例
   - 支持VAD（语音活动检测，Silero VAD），自动检测说话结束
   - 支持Hotwords（热词增强），可用CLI命令词库提升准确率
   - 支持标点恢复（punctuation）
   - 支持ITN（逆文本正则化）
4. **持续的维护和更新**：模型已更新到2025-09-09版本，Sherpa-ONNX非常活跃

---

## 五、技术栈选型

### 5.1 编程语言

| 语言 | 单exe | Sherpa绑定 | Windows API | 开发效率 | 最终评分 |
|------|-------|-----------|-------------|----------|----------|
| **Go** | ✅ | ✅ 官方 | ✅ `x/sys/windows` | ⭐⭐⭐⭐ | **🥇** |
| Rust | ✅ | ✅ crates.io | ✅ winapi | ⭐⭐⭐ | 🥈 |
| C# | 需AOT | ✅ 官方 | ✅ .NET原生 | ⭐⭐⭐⭐ | 🥉 |
| Python | ❌ 大 | ✅ | ❌ 需第三方 | ⭐⭐⭐⭐⭐ | ❌ |

### 5.2 最终决策：**Go 语言**

理由：
1. Sherpa-ONNX有**官方Go语言绑定**，提供完整的麦克风识别示例（`real-time-speech-recognition-from-microphone`）
2. 编译为**单个独立exe**，零运行时依赖，分发给用户非常方便
3. Go的Windows系统调用路径成熟：`golang.org/x/sys/windows` 可直接调用 Win32 API
4. goroutine天然适合并发：热键监听、音频采集、ASR推理可以并行运行
5. Go交叉编译方便，未来可扩展到Linux/macOS
6. Go生态有成熟的CLI框架（cobra）、剪贴板库等

### 5.2 Sherpa-ONNX Go 包

- Windows包: `github.com/k2-fsa/sherpa-onnx-go-windows` (支持 x86_64, x86)
- 统一包: `github.com/k2-fsa/sherpa-onnx-go`
- 需要开启 CGO（因为底层调用C库）
- 预编译的 `.dll` 和 `.lib` 文件可直接从GitHub Releases下载

### 5.3 其他技术组件

| 功能 | Go库/方案 | 说明 |
|------|-----------|------|
| 全局热键 | `golang.org/x/sys/windows` + Win32 `RegisterHotKey` | 系统级热键注册 |
| 按键检测 | Win32 `GetAsyncKeyState` | PTT（push-to-talk）按键状态 |
| 音频采集 | Sherpa-ONNX内置（WASAPI） | 已在Go示例中实现，无需额外库 |
| ASR推理 | Sherpa-ONNX Go SDK | 加载SenseVoice ONNX模型 |
| LLM修正 | HTTP调用（Claude API / OpenAI API / 本地Ollama） | Go的 `net/http` 足够 |
| 剪贴板操作 | `golang.org/x/sys/windows` + Win32 Clipboard API | 或使用 `github.com/atotto/clipboard` |
| CLI输入模拟 | Win32 `keybd_event` 或剪贴板+模拟Ctrl+V | 推荐剪贴板方案 |
| 配置文件 | `github.com/spf13/viper` | 热键、模型路径、LLM配置 |
| 系统托盘 | `github.com/getlantern/systray`（可选） | 后台常驻的托盘图标 |
| 日志 | `go.uber.org/zap` | 结构化日志 |

---

## 六、关键技术难点验证

### 6.1 全局热键（已验证可行）

Win32 `RegisterHotKey` API 可注册全局热键，即使应用不在前台也能捕获。Go通过 `golang.org/x/sys/windows` 调用：

```go
// 注册 F2 作为热键 (示例)
const MOD_NOREPEAT = 0x4000
user32.RegisterHotKey(0, 1, MOD_NOREPEAT, 0x71) // 0x71 = F2
```

### 6.2 按键松开检测（已验证可行）

使用 `GetAsyncKeyState` 轮询按键状态，检测 KeyUp 事件：

```go
for {
    state, _ := user32.GetAsyncKeyState(vkCode)
    if state&0x8000 == 0 && wasPressed {
        // Key released - trigger stop recording
    }
    wasPressed = state&0x8000 != 0
}
```

### 6.3 实时音频采集+ASR（Sherpa-ONNX官方示例已实现）

Sherpa-ONNX Go SDK 的 `real-time-speech-recognition-from-microphone` 示例完整覆盖了：
- WASAPI 麦克风采集
- 音频流送ASR引擎
- 实时返回识别结果

### 6.4 剪贴板+粘贴（已验证可行）

Windows剪贴板API在Go中可调用，结合模拟Ctrl+V：

```go
// 1. 写入剪贴板
clipboard.WriteAll(correctedText)
// 2. 模拟 Ctrl+V
keybd_event(0x11, 0, 0, 0)           // Ctrl down
keybd_event(0x56, 0, 0, 0)           // V down
keybd_event(0x56, 0, KEYEVENTF_KEYUP, 0) // V up
keybd_event(0x11, 0, KEYEVENTF_KEYUP, 0)  // Ctrl up
```

### 6.5 Hotwords 热词增强（Sherpa-ONNX 已验证可行）

SenseVoice模型本身不支持Hotwords（Hotwords只支持transducer模型），但有替代方案：
- **Post-ASR修正**：在ASR输出后用LLM做热词修正（将相似发音替换为正确的CLI术语）
- 或者使用支持Hotwords的transducer模型做第二遍识别

---

## 七、整体架构设计

```
┌──────────────────────────────────────────────────────────┐
│                     audio-input.exe                       │
│  (Go 编译的单文件，运行在系统托盘)                         │
│                                                          │
│  ┌──────────┐   ┌──────────┐   ┌──────────┐   ┌───────┐ │
│  │ 热键监听  │   │ 音频采集  │   │ ASR引擎  │   │ LLM   │ │
│  │ 🔑       │──▶│ 🎤       │──▶│ 🧠       │──▶│ 🔧    │ │
│  │ Win32    │   │ WASAPI   │   │ SenseVoice│  │修正器 │ │
│  │ Hotkey   │   │ 麦克风   │   │ +Hotwords│   │       │ │
│  └──────────┘   └──────────┘   └──────────┘   └───┬───┘ │
│                                                    │     │
│  ┌──────────┐                                     │     │
│  │ CLI 输入  │◀────────────────────────────────────┘     │
│  │ 📋 粘贴   │                                           │
│  │ Clipbrd  │                                           │
│  └──────────┘                                           │
│                                                          │
│  配置: config.yaml (热键、模型路径、LLM配置、热词表)      │
│  日志: %APPDATA%/audio-input/logs/                       │
└──────────────────────────────────────────────────────────┘
```

---

## 八、参考资源

| 资源 | 链接 |
|------|------|
| Sherpa-ONNX 官方文档 | https://k2-fsa.github.io/sherpa/onnx/ |
| Sherpa-ONNX GitHub | https://github.com/k2-fsa/sherpa-onnx |
| Sherpa-ONNX SenseVoice | https://k2-fsa.github.io/sherpa/onnx/sense-voice/index.html |
| Sherpa-ONNX Go API | https://k2-fsa.github.io/sherpa/onnx/go-api/index.html |
| Sherpa-ONNX Windows安装 | https://k2-fsa.github.io/sherpa/onnx/install/windows.html |
| Sherpa-ONNX Go Windows包 | https://pkg.go.dev/github.com/k2-fsa/sherpa-onnx-go-windows |
| SenseVoice原始模型 | https://github.com/FunAudioLLM/SenseVoice |
| SenseVoice on ModelScope | https://www.modelscope.cn/models/iic/SenseVoiceSmall |
| Whisper.cpp | https://github.com/ggerganov/whisper.cpp |
| FunASR | https://github.com/modelscope/FunASR |
