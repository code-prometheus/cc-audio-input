//! 音频录制 — cpal WASAPI, 原生配置 + 智能降采样到 16kHz mono

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
        host.default_input_device().context("未找到麦克风")?
    } else {
        let mut devices = host.input_devices()?;
        devices.nth(config.device_id as usize).context(format!("设备ID={}", config.device_id))?
    };

    let dev_name = device.name()?;
    info!("🎤 设备: {}", dev_name);

    // ★ 用设备原生配置采集, 然后在回调中智能降采样到 16kHz mono
    let def = device.default_input_config()?;
    let in_sr = def.sample_rate().0;
    let in_ch = def.channels() as usize;
    info!(" 设备默认 {}Hz {}ch, cpal将自动转换为16kHz mono", in_sr, in_ch);

    let buf = buffer.clone();
    let err_flag = Arc::new(AtomicBool::new(false));
    let err_msg: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));
    let is_rec = is_recording.clone();

    let stream = {
        let buf = buf.clone();
        let err_flag_c = err_flag.clone();
        let err_msg_c = err_msg.clone();
        device.build_input_stream(
            &def.into(),
            move |data: &[f32], _: &cpal::InputCallbackInfo| {
                if !is_rec.load(Ordering::SeqCst) { return; }
                let mut out = buf.lock().unwrap();
                // 智能降采样: 原生采样率 → 16000, 只取第1声道
                if in_sr == 16000 && in_ch == 1 {
                    out.extend_from_slice(data);
                    return;
                }
                // data 是 interleaved: [ch0,ch1,ch0,ch1,...]
                // ratio = 每帧需跳过的比例, 比如 48000/16000 = 3
                let ratio = in_sr as usize / 16000;
                for frame in data.chunks(in_ch).step_by(ratio) {
                    out.push(frame[0]); // 只取第1声道
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
    info!("🔴 录音中 ({}Hz {}ch → 16kHz mono)", in_sr, in_ch);

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
    let duration = samples as f64 / 16000.0;
    info!("⏹️ 录音结束: {} 采样点, {:.1}s", samples, duration);

    Ok(())
}
