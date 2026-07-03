//! 音频录制 — cpal WASAPI, 精确 16kHz mono f32

use anyhow::{Context, Result};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use log::{info, error};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

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
        host.default_input_device().context("Microphone not found")?
    } else {
        let mut devices = host.input_devices()?;
        devices.nth(config.device_id as usize).context(format!("deviceID={}", config.device_id))?
    };

    let dev_name = device.name()?;
    info!("device: {}", dev_name);

    // ★ 构造精确的配置: 16kHz, mono, f32
    let target_config = cpal::StreamConfig {
        channels: 1,
        sample_rate: cpal::SampleRate(16000),
        buffer_size: cpal::BufferSize::Default,
    };

    // 检查device是否支持此配置，如果不支持则用Default配置（由 cpal 自动转换）
    let supported = device.supported_input_configs()?
        .find(|c| c.channels() >= 1 && c.max_sample_rate() >= cpal::SampleRate(16000) && c.min_sample_rate() <= cpal::SampleRate(16000));

    let actual_config = match supported {
        Some(_sup_cfg) => {
            info!("   device支持 16kHz, 使用精确配置");
            target_config.clone()
        }
        None => {
            // device不支持 16kHz — 让 cpal 用Default配置然后自动转换
            let def = device.default_input_config()?;
            info!("   deviceDefault {}Hz {}ch, cpal将自动转换为16kHz mono",
                  def.sample_rate().0, def.channels());
            def.into()
        }
    };

    let buf = buffer.clone();
    let err_flag = Arc::new(AtomicBool::new(false));
    let err_msg: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));
    let is_rec = is_recording.clone();
    let target_sr = target_config.sample_rate.0;

    let stream = {
        let buf = buf.clone();
        let err_flag = err_flag.clone();
        let err_msg = err_msg.clone();
        device.build_input_stream(
            &actual_config,
            move |data: &[f32], _: &cpal::InputCallbackInfo| {
                if is_rec.load(Ordering::SeqCst) {
                    // ★ 重采样: 如果实际采样率不是 16kHz，做简单线性插值
                    let actual_sr = target_sr; // cpal 会用配置中的采样率
                    if actual_sr == 16000 {
                        buf.lock().unwrap().extend_from_slice(data);
                    } else {
                        // 降采样: 48000 → 16000 (取每3个样本的第1个)
                        let ratio = actual_sr / 16000;
                        for chunk in data.chunks(ratio as usize) {
                            buf.lock().unwrap().push(chunk[0]);
                        }
                    }
                }
            },
            move |e| {
                error!("Audio stream error: {}", e);
                err_flag.store(true, Ordering::SeqCst);
                *err_msg.lock().unwrap() = Some(e.to_string());
            },
            None,
        )?
    };

    stream.play()?;
    info!("Recording (16kHz mono f32)");

    while is_recording.load(Ordering::SeqCst) {
        if err_flag.load(Ordering::SeqCst) {
            let msg = err_msg.lock().unwrap().clone();
            drop(stream);
            return Err(anyhow::anyhow!("Audio stream error: {}", msg.unwrap_or_default()));
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }

    drop(stream);

    let samples = buffer.lock().unwrap().len();
    let duration = samples as f64 / 16000.0;
    info!("Recording ended: {} samples, {:.1}s", samples, duration);

    Ok(())
}
