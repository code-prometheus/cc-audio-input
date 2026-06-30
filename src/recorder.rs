//! 音频录制模块 — cpal WASAPI 采集

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

/// 阻塞录音直到 is_recording 变为 false
pub fn record_blocking(
    config: &RecorderConfig,
    is_recording: Arc<AtomicBool>,
    buffer: &Arc<Mutex<Vec<f32>>>,
) -> Result<()> {
    buffer.lock().unwrap().clear();

    let host = cpal::default_host();
    let device = if config.device_id < 0 {
        host.default_input_device()
            .context("未找到麦克风")?
    } else {
        let devices = host.input_devices()?;
        let mut found = None;
        for (i, d) in devices.enumerate() {
            if i == config.device_id as usize { found = Some(d); break; }
        }
        found.context(format!("设备ID={}", config.device_id))?
    };

    info!("🎤 设备: {}", device.name()?);

    let supported_config = {
        let mut configs = device.supported_input_configs()?;
        let target = configs
            .find(|c| {
                c.min_sample_rate() <= cpal::SampleRate(config.sample_rate)
                && c.max_sample_rate() >= cpal::SampleRate(config.sample_rate)
                && c.channels() >= config.channels
            })
            .context("不支持16kHz单声道")?;
        target.with_sample_rate(cpal::SampleRate(config.sample_rate))
    };

    let buf = buffer.clone();
    let err_flag = Arc::new(AtomicBool::new(false));
    let err_msg: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));
    let is_rec = is_recording.clone();

    let stream = {
        let buf = buf.clone();
        let err_flag = err_flag.clone();
        let err_msg = err_msg.clone();
        device.build_input_stream(
            &supported_config.config(),
            move |data: &[f32], _: &cpal::InputCallbackInfo| {
                if is_rec.load(Ordering::SeqCst) {
                    buf.lock().unwrap().extend_from_slice(data);
                }
            },
            move |e| {
                error!("音频流错误: {}", e);
                err_flag.store(true, Ordering::SeqCst);
                *err_msg.lock().unwrap() = Some(e.to_string());
            },
            None,
        )?
    };

    stream.play()?;
    info!("🔴 录音中 ({}Hz)", config.sample_rate);

    while is_recording.load(Ordering::SeqCst) {
        if err_flag.load(Ordering::SeqCst) {
            let msg = err_msg.lock().unwrap().clone();
            drop(stream);
            return Err(anyhow::anyhow!("录音流错误: {}", msg.unwrap_or_default()));
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }

    drop(stream);

    let samples = buffer.lock().unwrap().len();
    let duration = samples as f64 / config.sample_rate as f64;
    info!("⏹️  录音结束: {} 采样点, {:.1}s", samples, duration);

    Ok(())
}
