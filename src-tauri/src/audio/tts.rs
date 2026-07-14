use anyhow::Result;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

/// TTS via Piper — ONNX neural TTS, cross-platform, offline, genuinely good quality.
/// Text is piped to stdin; Piper writes a WAV file; we play it via cpal.
pub struct TtsClient;

impl TtsClient {
    pub fn new(_port: u16) -> Self { Self }

    pub async fn speak(&self, text: &str, app: &tauri::AppHandle) -> Result<()> {
        use crate::monitor::{emit_debug_event, new_event_log};
        // Create a throw-away log since emit_debug_event requires one — events go live via app_handle
        let dummy_log = new_event_log();
        let clean = clean_for_speech(text);
        if clean.trim().is_empty() { return Ok(()); }

        let binary = match find_piper() {
            Ok(b) => b,
            Err(e) => { emit_debug_event(app, &dummy_log, "[AUDIO]", format!("TTS: {e}")).await; return Err(e); }
        };
        let model = match find_tts_model() {
            Ok(m) => m,
            Err(e) => { emit_debug_event(app, &dummy_log, "[AUDIO]", format!("TTS: {e}")).await; return Err(e); }
        };
        emit_debug_event(app, &dummy_log, "[AUDIO]", format!(
            "TTS: {} chars → {}", clean.len(), binary.file_name().unwrap_or_default().to_string_lossy()
        )).await;
        let tmp = std::env::temp_dir().join("proactive_tts.wav");

        // Piper reads text from stdin, writes WAV to --output_file.
        // Run from piper/ so relative libs (and espeak-ng-data) resolve on all OSes.
        let piper_dir = binary.parent().map(|p| p.to_path_buf()).unwrap_or_else(|| binary.clone());
        let mut cmd = tokio::process::Command::new(&binary);
        cmd.args(["--model", model.to_str().unwrap_or(""),
                   "--output_file", tmp.to_str().unwrap_or(""),
                   "--sentence_silence", "0.1"])
            .current_dir(&piper_dir)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null());
        #[cfg(target_os = "windows")]
        {
            // Exe dir is searched for DLLs; also prepend PATH for any helper tools
            let current = std::env::var("PATH").unwrap_or_default();
            cmd.env("PATH", format!("{};{}", piper_dir.display(), current));
        }
        #[cfg(target_os = "linux")]
        {
            let current = std::env::var("LD_LIBRARY_PATH").unwrap_or_default();
            let path = if current.is_empty() {
                piper_dir.display().to_string()
            } else {
                format!("{}:{}", piper_dir.display(), current)
            };
            cmd.env("LD_LIBRARY_PATH", path);
        }
        #[cfg(target_os = "macos")]
        {
            let current = std::env::var("DYLD_LIBRARY_PATH").unwrap_or_default();
            let path = if current.is_empty() {
                piper_dir.display().to_string()
            } else {
                format!("{}:{}", piper_dir.display(), current)
            };
            cmd.env("DYLD_LIBRARY_PATH", path);
        }
        let mut child = cmd.spawn()?;

        if let Some(mut stdin) = child.stdin.take() {
            use tokio::io::AsyncWriteExt;
            stdin.write_all(clean.as_bytes()).await?;
        }

        let status = child.wait().await?;
        if !status.success() {
            let msg = format!("piper exited {:?}", status.code());
            emit_debug_event(app, &dummy_log, "[AUDIO]", format!("TTS error: {msg}")).await;
            return Err(anyhow::anyhow!("{msg}"));
        }

        let wav = tokio::fs::read(&tmp).await?;
        let _ = tokio::fs::remove_file(&tmp).await;
        let (pcm, wav_rate, wav_channels) = wav_to_f32(&wav);
        emit_debug_event(app, &dummy_log, "[AUDIO]",
            format!("TTS: {} KB @ {}Hz {}ch — playing", wav.len() / 1024, wav_rate, wav_channels)).await;
        tokio::task::spawn_blocking(move || play_pcm_blocking(&pcm, wav_rate, wav_channels))
            .await
            .map_err(|e| anyhow::anyhow!("playback: {e}"))??;

        Ok(())
    }
}

fn find_piper() -> Result<std::path::PathBuf> {
    // 1. Standard sidecar naming (piper-x86_64-pc-windows-msvc.exe)
    if let Some(p) = crate::find_sidecar("piper") { return Ok(p); }

    // 2. Plain piper.exe — in case setup downloaded without the platform rename
    let bin_dir = crate::binaries_dir().join("piper");
    for name in &["piper.exe", "piper"] {
        let p = bin_dir.join(name);
        if p.exists() && p.metadata().map(|m| m.len() > 1024).unwrap_or(false) {
            return Ok(p);
        }
    }

    Err(anyhow::anyhow!(
        "piper not found. Tried binaries/piper/. Run: deno task setup"
    ))
}

fn find_tts_model() -> Result<std::path::PathBuf> {
    // In dev: models/tts/ next to project root; in release: relative to exe
    let candidates = [
        crate::binaries_dir().parent()
            .map(|p| p.join("models").join("tts").join(crate::constants::TTS_MODEL_FILE))
            .unwrap_or_default(),
        crate::binaries_dir()
            .parent().and_then(|p| p.parent())
            .map(|p| p.join("models").join("tts").join(crate::constants::TTS_MODEL_FILE))
            .unwrap_or_default(),
    ];
    candidates.into_iter()
        .find(|p| p.exists())
        .ok_or_else(|| anyhow::anyhow!(
            "piper voice model not found — run: deno task setup"
        ))
}

/// Strip markdown so it isn't read aloud as noise.
fn clean_for_speech(text: &str) -> String {
    let mut s = text.to_string();
    // Remove fenced code blocks
    while let Some(start) = s.find("```") {
        if let Some(rel) = s[start + 3..].find("```") {
            s = format!("{}{}", &s[..start], &s[start + 3 + rel + 3..]);
        } else { break; }
    }
    s = s.replace('`', "");
    s = s.replace("**", "").replace("__", "").replace('*', "").replace('_', " ");
    if let Ok(re) = regex::Regex::new(r"https?://\S+") {
        s = re.replace_all(&s, "").to_string();
    }
    // Remove <defer> tags if the parser somehow missed one
    if let Ok(re) = regex::Regex::new(r"(?s)<defer>.*?</defer>") {
        s = re.replace_all(&s, "").to_string();
    }
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Parse WAV header and return (samples, sample_rate, channels).
fn wav_to_f32(wav: &[u8]) -> (Vec<f32>, u32, u16) {
    if wav.len() < 44 { return (vec![], 44100, 1); }
    let channels    = u16::from_le_bytes([wav[22], wav[23]]);
    let sample_rate = u32::from_le_bytes([wav[24], wav[25], wav[26], wav[27]]);
    let samples = wav[44..].chunks_exact(2)
        .map(|b| i16::from_le_bytes([b[0], b[1]]) as f32 / 32768.0)
        .collect();
    (samples, sample_rate.max(8000), channels.max(1))
}

/// Resample from `src_rate` to `dst_rate` using linear interpolation.
/// Clamps to the last sample at the boundary rather than padding with silence,
/// so the very end of the buffer doesn't fade incorrectly.
fn resample(samples: &[f32], src_rate: u32, dst_rate: u32) -> Vec<f32> {
    if src_rate == dst_rate { return samples.to_vec(); }
    let last = samples.last().copied().unwrap_or(0.0);
    let ratio = src_rate as f64 / dst_rate as f64;
    let out_len = (samples.len() as f64 / ratio) as usize;
    (0..out_len).map(|i| {
        let pos = i as f64 * ratio;
        let idx = pos as usize;
        let frac = (pos - idx as f64) as f32;
        let s0 = samples.get(idx).copied().unwrap_or(last);
        let s1 = samples.get(idx + 1).copied().unwrap_or(last);
        s0 + (s1 - s0) * frac
    }).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── clean_for_speech ─────────────────────────────────────────────────────

    #[test]
    fn strips_fenced_code_block() {
        let input = "Here is some code:\n```rust\nlet x = 1;\n```\nThat's it.";
        let out = clean_for_speech(input);
        assert!(!out.contains("```"), "backtick fences should be removed");
        assert!(!out.contains("let x"), "code content should be removed");
        assert!(out.contains("Here is some code"), "surrounding text preserved");
        assert!(out.contains("That's it"), "trailing text preserved");
    }

    #[test]
    fn strips_inline_bold_italic() {
        let out = clean_for_speech("**Bold** and *italic* and __under__ and _em_");
        assert!(!out.contains("**"), "double-star removed");
        assert!(!out.contains("__"), "double-under removed");
        assert!(out.contains("Bold"), "bold text preserved");
        assert!(out.contains("italic"), "italic text preserved");
    }

    #[test]
    fn strips_url() {
        let out = clean_for_speech("Check https://example.com/foo?bar=1 for details");
        assert!(!out.contains("https://"), "URL removed");
        assert!(out.contains("Check"), "surrounding text preserved");
        assert!(out.contains("for details"), "trailing text preserved");
    }

    #[test]
    fn strips_defer_tag() {
        let out = clean_for_speech(
            r#"Nice chat. <defer>{"message":"follow up","after_minutes":60}</defer>"#
        );
        assert!(!out.contains("<defer>"), "defer tag removed");
        assert!(!out.contains("follow up"), "defer content removed");
        assert!(out.contains("Nice chat"), "main text preserved");
    }

    #[test]
    fn empty_or_whitespace_only_is_empty() {
        assert!(clean_for_speech("   ").trim().is_empty());
        assert!(clean_for_speech("```rust\ncode\n```").trim().is_empty());
    }

    // ── wav_to_f32 ───────────────────────────────────────────────────────────

    fn make_wav(sample_rate: u32, channels: u16, samples_i16: &[i16]) -> Vec<u8> {
        let data_bytes = (samples_i16.len() * 2) as u32;
        let mut w = Vec::new();
        // RIFF header
        w.extend_from_slice(b"RIFF");
        w.extend_from_slice(&(36 + data_bytes).to_le_bytes());
        w.extend_from_slice(b"WAVE");
        // fmt chunk
        w.extend_from_slice(b"fmt ");
        w.extend_from_slice(&16u32.to_le_bytes());  // chunk size
        w.extend_from_slice(&1u16.to_le_bytes());   // PCM
        w.extend_from_slice(&channels.to_le_bytes());
        w.extend_from_slice(&sample_rate.to_le_bytes());
        let byte_rate = sample_rate * channels as u32 * 2;
        w.extend_from_slice(&byte_rate.to_le_bytes());
        w.extend_from_slice(&(channels * 2).to_le_bytes());  // block align
        w.extend_from_slice(&16u16.to_le_bytes());  // bits per sample
        // data chunk
        w.extend_from_slice(b"data");
        w.extend_from_slice(&data_bytes.to_le_bytes());
        for &s in samples_i16 {
            w.extend_from_slice(&s.to_le_bytes());
        }
        w
    }

    #[test]
    fn wav_to_f32_reads_sample_rate_and_channels() {
        let wav = make_wav(22050, 1, &[0i16, 16384, -16384, 32767]);
        let (samples, rate, channels) = wav_to_f32(&wav);
        assert_eq!(rate, 22050, "sample rate from WAV header");
        assert_eq!(channels, 1, "mono channel count");
        assert_eq!(samples.len(), 4);
        // 32767 / 32768 ≈ 1.0 (within float precision)
        assert!((samples[3] - 1.0f32).abs() < 0.001, "max positive sample");
        assert!((samples[2] + 0.5f32).abs() < 0.001, "-16384 / 32768 = -0.5");
    }

    #[test]
    fn wav_too_short_returns_defaults() {
        let (samples, rate, channels) = wav_to_f32(&[0u8; 10]);
        assert!(samples.is_empty());
        assert_eq!(rate, 44100);
        assert_eq!(channels, 1);
    }

    #[test]
    fn wav_stereo_is_detected() {
        let wav = make_wav(48000, 2, &[100i16, 200]);
        let (_s, rate, channels) = wav_to_f32(&wav);
        assert_eq!(rate, 48000);
        assert_eq!(channels, 2);
    }

    // ── resample ─────────────────────────────────────────────────────────────

    #[test]
    fn resample_identity_same_rate() {
        let samples: Vec<f32> = (0..100).map(|i| i as f32 / 100.0).collect();
        let out = resample(&samples, 44100, 44100);
        assert_eq!(out.len(), samples.len(), "no-op when rates equal");
        for (a, b) in samples.iter().zip(out.iter()) {
            assert!((a - b).abs() < 1e-6, "values unchanged");
        }
    }

    #[test]
    fn resample_upsample_doubles_length() {
        // 22050 → 44100: ratio 0.5, output should be ~2× as long
        let samples = vec![0.0f32, 0.5, 1.0, 0.5, 0.0];
        let out = resample(&samples, 22050, 44100);
        // Allow ±1 for off-by-one in integer arithmetic
        let expected = samples.len() * 2 - 1;  // linear interp loses one at tail
        assert!((out.len() as i64 - expected as i64).abs() <= 1,
            "upsampled len {} ≈ expected {}", out.len(), expected);
    }

    #[test]
    fn resample_downsample_halves_length() {
        // 44100 → 22050: ratio 2, output should be ~½ as long
        let samples: Vec<f32> = (0..100).map(|i| i as f32 / 100.0).collect();
        let out = resample(&samples, 44100, 22050);
        assert!((out.len() as i64 - 50i64).abs() <= 1,
            "downsampled len {} ≈ 50", out.len());
    }

    #[test]
    fn resample_empty_input_returns_empty() {
        let out = resample(&[], 22050, 44100);
        assert!(out.is_empty());
    }

    #[test]
    fn resample_piper_to_device_doubles_correctly() {
        // The critical fix: 22050 (piper) → 44100 (typical device)
        // A constant signal should remain constant after resampling
        let samples = vec![0.5f32; 441]; // 441 samples at 22050 Hz = 20ms
        let out = resample(&samples, 22050, 44100);
        // Every output sample should be ~0.5 (constant signal)
        for (i, &v) in out.iter().enumerate() {
            assert!((v - 0.5f32).abs() < 1e-4,
                "sample {i}: expected 0.5, got {v}");
        }
    }
}

fn play_pcm_blocking(samples: &[f32], src_rate: u32, src_channels: u16) -> Result<()> {
    if samples.is_empty() { return Ok(()); }
    let host   = cpal::default_host();
    let device = host.default_output_device()
        .ok_or_else(|| anyhow::anyhow!("no output device"))?;
    let config: cpal::StreamConfig = device.default_output_config()?.into();
    let dst_rate     = config.sample_rate.0;
    let dst_channels = config.channels as usize;

    // 1. Resample to device sample rate
    let resampled = resample(samples, src_rate, dst_rate);

    // 2. Upmix channels: mono→stereo duplicates each sample for L and R
    let samples = Arc::new(if src_channels as usize == dst_channels {
        resampled
    } else {
        resampled.iter()
            .flat_map(|&s| std::iter::repeat(s).take(dst_channels))
            .collect()
    });
    let pos      = Arc::new(AtomicUsize::new(0));
    let (tx, rx) = std::sync::mpsc::sync_channel::<()>(1);
    let sc = samples.clone();
    let pc = pos.clone();
    let stream = device.build_output_stream::<f32, _, _>(
        &config,
        move |out: &mut [f32], _| {
            let p    = pc.load(Ordering::Relaxed);
            let copy = sc.len().saturating_sub(p).min(out.len());
            out[..copy].copy_from_slice(&sc[p..p + copy]);
            for s in &mut out[copy..] { *s = 0.0; }
            pc.fetch_add(copy, Ordering::Relaxed);
            if copy == 0 { let _ = tx.try_send(()); }
        },
        |e| eprintln!("[TTS] playback error: {e}"),
        None,
    )?;
    stream.play()?;
    let _ = rx.recv();
    Ok(())
}
