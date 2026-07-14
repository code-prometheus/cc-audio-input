//! 音频录制 — cpal WASAPI, 原生配置 + 高质量降采样到 16kHz mono
use anyhow::{Context, Result};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use log::{info, error};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
// 引入专业的重采样库
use rubato::{FastFixedIn, Resampler};

#[derive(Debug, Clone)]
pub struct RecorderConfig {
    pub sample_rate: u32,
    pub device_id: i32,
    pub channels: u16,
}

pub fn record_blocking(
    config: &RecorderConfig,
    is_recording: Arc<AtomicBool>,
    buffer: &Arc<Mutex<Vec<f32>>>,
) -> Result<()> {
    buffer.lock().unwrap().clear();
    let host = cpal::default_host();

    let device = if config.device_id < 0 {
        host.default_input_device().context("未找到麦克风")?
    } else {
        let mut devices = host.input_devices()?;
        devices.nth(config.device_id as usize).context(format!("设备ID={}", config.device_id))?
    };

    let dev_name = device.name()?;
    info!("🎤 设备: {}", dev_name);

    let def = device.default_input_config()?;
    let in_sr = def.sample_rate().0;
    let in_ch = def.channels() as usize;
    info!(" 设备默认 {}Hz {}ch", in_sr, in_ch);

    let buf = buffer.clone();
    let err_flag = Arc::new(AtomicBool::new(false));
    let err_msg: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));
    let is_rec = is_recording.clone();

    // 初始化高质量重采样器 (如果输入不是 16kHz)
    // 用 Arc<Mutex<>> 包装以便闭包和外部 flush 共享
    let resampler: Arc<Mutex<Option<FastFixedIn<f32>>>> = if in_sr != 16000 {
        let ratio = 16000.0 / in_sr as f64;
        let r = FastFixedIn::<f32>::new(
            ratio,
            1.0,
            rubato::PolynomialDegree::Cubic,
            10,
            1024,
        ).context("初始化 rubato 重采样器失败")?;
        Arc::new(Mutex::new(Some(r)))
    } else {
        Arc::new(Mutex::new(None))
    };

    let stream = {
        let buf = buf.clone();
        let err_flag_c = err_flag.clone();
        let err_msg_c = err_msg.clone();
        let is_rec = is_rec.clone();
        let resampler = resampler.clone();

        device.build_input_stream(
            &def.into(),
            move |data: &[f32], _: &cpal::InputCallbackInfo| {
                if !is_rec.load(Ordering::SeqCst) { return; }

                let mut out = buf.lock().unwrap();

                // 1. 多声道安全混合 (Downmix) -> 转为单声道
                let mut mono_data = Vec::with_capacity(data.len() / in_ch);
                if in_ch == 1 {
                    mono_data.extend_from_slice(data);
                } else {
                    for frame in data.chunks(in_ch) {
                        let sum: f32 = frame.iter().sum();
                        mono_data.push(sum / in_ch as f32);
                    }
                }

                // 2. 智能降采样 -> 16kHz
                if in_sr == 16000 {
                    out.extend_from_slice(&mono_data);
                } else {
                    let mut r = resampler.lock().unwrap();
                    if let Some(ref mut resampler) = *r {
                        let in_buffers: &[&[f32]] = &[&mono_data];
                        let mut out_buffers = vec![Vec::new()];
                        if resampler.process_into_buffer(in_buffers, &mut out_buffers, None).is_ok() {
                            out.extend_from_slice(&out_buffers[0]);
                        }
                    }
                }
            },
            move |e| {
                error!("音频流错误: {}", e);
                err_flag_c.store(true, Ordering::SeqCst);
                *err_msg_c.lock().unwrap() = Some(e.to_string());
            },
            None,
        )?
    };

    stream.play()?;
    info!("🔴 录音中 ({}Hz {}ch → 16kHz mono, 使用 rubato 高质量重采样)", in_sr, in_ch);

    while is_recording.load(Ordering::SeqCst) {
        if err_flag.load(Ordering::SeqCst) {
            let msg = err_msg.lock().unwrap().clone();
            drop(stream);
            return Err(anyhow::anyhow!("录音流错误: {}", msg.unwrap_or_default()));
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }

    drop(stream);

    // 处理重采样器尾部残留数据 (Flush)
    {
        let mut r = resampler.lock().unwrap();
        if let Some(ref mut resampler) = *r {
            let mut out = buf.lock().unwrap();
            let empty_in: &[&[f32]] = &[&[]];
            let mut out_buffers = vec![Vec::new()];
            let _ = resampler.process_into_buffer(empty_in, &mut out_buffers, None);
            out.extend_from_slice(&out_buffers[0]);
        }
    }

    let samples = buffer.lock().unwrap().len();
    let duration = samples as f64 / 16000.0;
    info!("⏹️ 录音结束: {} 采样点, {:.1}s", samples, duration);

    Ok(())
}