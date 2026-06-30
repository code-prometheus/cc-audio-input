//! Sherpa-ONNX ASR 引擎 (SenseVoice FFI 绑定)
//!
//! 通过 `sherpa-onnx-c-api.dll` 调用 C API，加载 SenseVoice int8 模型。

use anyhow::{Context, Result};
use log::{info, warn};
use std::ffi::{c_char, c_float, c_int, c_void, CStr, CString};
use std::path::Path;

// ── FFI 类型定义 (对应 sherpa-onnx/c-api/c-api.h) ──

type SherpaOnnxOfflineRecognizer = c_void;
type SherpaOnnxOfflineStream = c_void;

#[repr(C)]
struct SherpaOnnxOfflineSenseVoiceModelConfig {
    model: *const c_char,
}

#[repr(C)]
struct SherpaOnnxOfflineRecognizerConfig {
    model_config: SherpaOnnxOfflineModelConfig,
    decoding_method: *const c_char,
    max_active_paths: c_int,
    hotwords_file: *const c_char,
    hotwords_score: c_float,
    num_threads: c_int,
    provider: *const c_char,
    enable_endpoint: c_int,
    rule1_min_trailing_silence: c_float,
    rule2_min_trailing_silence: c_float,
    rule3_min_utterance_length: c_float,
}

#[repr(C)]
union SherpaOnnxOfflineModelConfig {
    sense_voice: std::mem::ManuallyDrop<SherpaOnnxOfflineSenseVoiceModelConfig>,
    _pad: [u8; 1024],
}

#[repr(C)]
struct SherpaOnnxOfflineStreamResult {
    text: *const c_char,
    json: *const c_char,
    lang: *const c_char,
    audio_duration_s: c_float,
    processing_duration_s: c_float,
    tokens: *const c_char,
    timestamps: *const c_char,
}

// ── DLL 函数指针 ──

type FnCreateOfflineRecognizer = unsafe extern "C" fn(
    *const SherpaOnnxOfflineRecognizerConfig,
) -> *const SherpaOnnxOfflineRecognizer;

type FnDestroyOfflineRecognizer = unsafe extern "C" fn(*const SherpaOnnxOfflineRecognizer);

type FnCreateOfflineStream = unsafe extern "C" fn(
    *const SherpaOnnxOfflineRecognizer,
) -> *const SherpaOnnxOfflineStream;

type FnDestroyOfflineStream = unsafe extern "C" fn(*const SherpaOnnxOfflineStream);

type FnAcceptWaveformOffline = unsafe extern "C" fn(
    *const SherpaOnnxOfflineStream,
    c_int,
    *const c_float,
    c_int,
);

type FnDecodeOfflineStream = unsafe extern "C" fn(
    *const SherpaOnnxOfflineRecognizer,
    *const SherpaOnnxOfflineStream,
);

type FnGetOfflineStreamResult = unsafe extern "C" fn(
    *const SherpaOnnxOfflineStream,
) -> *const SherpaOnnxOfflineStreamResult;

type FnDestroyOfflineStreamResult = unsafe extern "C" fn(*const SherpaOnnxOfflineStreamResult);

type FnFree = unsafe extern "C" fn(*mut c_void);

pub struct AsrEngine {
    recognizer: *const SherpaOnnxOfflineRecognizer,
    dll: libloading::Library,
    model_dir: std::path::PathBuf,
}

// SAFETY: Sherpa-ONNX 的 recognizer 指针是线程安全的（内部使用互斥锁）
// DLL 函数调用也是线程安全的
unsafe impl Send for AsrEngine {}
unsafe impl Sync for AsrEngine {}

impl AsrEngine {
    /// 创建 ASR 引擎 (加载 SenseVoice 模型)
    pub fn new(model_dir: &Path) -> Result<Self> {
        // 查找 sherpa-onnx-c-api.dll
        let dll_paths = [
            Path::new("sherpa-onnx-c-api.dll"),
            Path::new("sherpa_dll/sherpa-onnx-c-api.dll"),
            Path::new("./sherpa-onnx-c-api.dll"),
            Path::new("target/release/sherpa-onnx-c-api.dll"),
            Path::new("../sherpa-onnx-c-api.dll"),
        ];

        let dll = dll_paths.iter()
            .find(|p| p.exists())
            .map(|p| unsafe { libloading::Library::new(p) })
            .ok_or_else(|| anyhow::anyhow!("找不到 sherpa-onnx-c-api.dll"))?
            .context("加载 sherpa-onnx-c-api.dll 失败")?;

        let model_path = model_dir.join("model.int8.onnx");
        let tokens_path = model_dir.join("tokens.txt");

        if !model_path.exists() {
            warn!("SenseVoice 模型不存在: {:?}", model_path);
            warn!("请下载: https://github.com/k2-fsa/sherpa-onnx/releases/download/asr-models/sherpa-onnx-sense-voice-zh-en-ja-ko-yue-int8-2025-09-09.tar.bz2");
            return Err(anyhow::anyhow!("模型文件未找到"));
        }

        info!("🔧 加载 SenseVoice 模型: {:?}", model_path);

        let model_path_c = CString::new(model_path.to_str().unwrap()).unwrap();
        let decoding_method = CString::new("greedy_search").unwrap();
        let provider = CString::new("cpu").unwrap();
        let empty = CString::new("").unwrap();

        let config = SherpaOnnxOfflineRecognizerConfig {
            model_config: SherpaOnnxOfflineModelConfig {
                sense_voice: std::mem::ManuallyDrop::new(
                    SherpaOnnxOfflineSenseVoiceModelConfig {
                    model: model_path_c.as_ptr(),
                }),
            },
            decoding_method: decoding_method.as_ptr(),
            max_active_paths: 4,
            hotwords_file: empty.as_ptr(),
            hotwords_score: 1.5,
            num_threads: 4,
            provider: provider.as_ptr(),
            enable_endpoint: 0,
            rule1_min_trailing_silence: 2.4,
            rule2_min_trailing_silence: 1.2,
            rule3_min_utterance_length: 20.0,
        };

        unsafe {
            let create: libloading::Symbol<FnCreateOfflineRecognizer> =
                dll.get(b"SherpaOnnxCreateOfflineRecognizer")?;
            let recognizer = create(&config);
            if recognizer.is_null() {
                return Err(anyhow::anyhow!("创建离线识别器失败"));
            }
            info!("✅ SenseVoice 模型加载成功");
            Ok(Self {
                recognizer,
                dll,
                model_dir: model_dir.to_path_buf(),
            })
        }
    }

    /// 执行语音识别
    pub fn recognize(&self, audio_data: &[f32], sample_rate: u32) -> Result<String> {
        unsafe {
            let create_stream: libloading::Symbol<FnCreateOfflineStream> =
                self.dll.get(b"SherpaOnnxCreateOfflineStream")?;
            let accept: libloading::Symbol<FnAcceptWaveformOffline> =
                self.dll.get(b"SherpaOnnxAcceptWaveformOffline")?;
            let decode: libloading::Symbol<FnDecodeOfflineStream> =
                self.dll.get(b"SherpaOnnxDecodeOfflineStream")?;
            let get_result: libloading::Symbol<FnGetOfflineStreamResult> =
                self.dll.get(b"SherpaOnnxGetOfflineStreamResult")?;
            let destroy_result: libloading::Symbol<FnDestroyOfflineStreamResult> =
                self.dll.get(b"SherpaOnnxDestroyOfflineStreamResult")?;
            let destroy_stream: libloading::Symbol<FnDestroyOfflineStream> =
                self.dll.get(b"SherpaOnnxDestroyOfflineStream")?;

            let stream = create_stream(self.recognizer);
            if stream.is_null() {
                return Err(anyhow::anyhow!("创建识别流失败"));
            }

            accept(stream, sample_rate as c_int, audio_data.as_ptr(), audio_data.len() as c_int);
            decode(self.recognizer, stream);

            let result_ptr = get_result(stream);
            if result_ptr.is_null() {
                destroy_stream(stream);
                return Err(anyhow::anyhow!("获取识别结果失败"));
            }

            let text = CStr::from_ptr((*result_ptr).text)
                .to_str()
                .unwrap_or("")
                .to_string();

            destroy_result(result_ptr);
            destroy_stream(stream);

            Ok(text)
        }
    }
}

impl Drop for AsrEngine {
    fn drop(&mut self) {
        unsafe {
            if !self.recognizer.is_null() {
                let destroy: libloading::Symbol<FnDestroyOfflineRecognizer> =
                    self.dll.get(b"SherpaOnnxDestroyOfflineRecognizer").unwrap();
                destroy(self.recognizer);
            }
        }
    }
}
