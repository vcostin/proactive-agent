//! Diagnose mic capture the same way the app does after the PipeWire fix.
//! Run: cargo run --example mic_probe
//! Exit 0 = frames delivered; non-zero = capture broken.

use cpal::traits::{DeviceTrait, StreamTrait};
use proactive_agent_lib::audio::{frame_rms, quiet_backend_probe_noise, resolve_input_device};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::time::Duration;

fn main() {
    quiet_backend_probe_noise();

    let host = cpal::default_host();
    println!("default_host={:?}", host.id());

    let device = match resolve_input_device(&host) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("FAIL: {e}");
            std::process::exit(2);
        }
    };
    let name = device.name().unwrap_or_else(|_| "?".into());
    println!("using_input={name}");

    let supported = match device.default_input_config() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("FAIL: default_input_config: {e}");
            std::process::exit(3);
        }
    };
    println!("config={supported:?}");
    let channels = supported.channels() as usize;
    let config: cpal::StreamConfig = supported.into();

    let frames = Arc::new(AtomicU32::new(0));
    let peak = Arc::new(AtomicU32::new(0));
    let f2 = frames.clone();
    let p2 = peak.clone();

    let stream = match device.build_input_stream(
        &config,
        move |data: &[f32], _| {
            f2.fetch_add(1, Ordering::Relaxed);
            let rms = frame_rms(data, channels);
            let bits = rms.to_bits();
            let cur = p2.load(Ordering::Relaxed);
            if f32::from_bits(cur) < rms {
                p2.store(bits, Ordering::Relaxed);
            }
        },
        |e| eprintln!("stream err: {e}"),
        None,
    ) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("FAIL: build_input_stream: {e}");
            std::process::exit(4);
        }
    };

    if let Err(e) = stream.play() {
        eprintln!("FAIL: play: {e}");
        std::process::exit(5);
    }

    println!("recording 2s — speak into the mic…");
    std::thread::sleep(Duration::from_secs(2));

    let n = frames.load(Ordering::Relaxed);
    let peak_rms = f32::from_bits(peak.load(Ordering::Relaxed));
    println!("frames={n} peak_rms={peak_rms:.6} vad_threshold=0.005");

    if n == 0 {
        eprintln!("FAIL: no frames (stream broken)");
        std::process::exit(6);
    }
    if peak_rms < 0.0001 {
        eprintln!("FAIL: peak_rms≈0 (wrong device / muted / exclusive lock)");
        std::process::exit(7);
    }
    if peak_rms < 0.005 {
        println!("WARN: peak_rms below VAD (ambient only?) — capture still works");
    }
    println!("OK: capture path opens and delivers frames");
}
