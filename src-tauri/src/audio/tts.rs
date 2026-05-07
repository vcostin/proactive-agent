use anyhow::Result;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

/// TTS via Piper — ONNX neural TTS, cross-platform, offline, genuinely good quality.
/// Text is piped to stdin; Piper writes a WAV file; we play it via cpal.
pub struct TtsClient;

impl TtsClient {
    pub fn new(_port: u16) -> Self { Self }

    pub async fn speak(&self, text: &str) -> Result<()> {
        let clean = clean_for_speech(text);
        if clean.trim().is_empty() { return Ok(()); }

        let binary = find_piper().map_err(|e| {
            eprintln!("[TTS] binary not found: {e}"); e
        })?;
        let model = find_tts_model().map_err(|e| {
            eprintln!("[TTS] model not found: {e}"); e
        })?;
        eprintln!("[TTS] binary={} model={}", binary.display(), model.display());
        let tmp    = std::env::temp_dir().join("proactive_tts.wav");

        // Piper reads text from stdin, writes WAV to --output_file
        let mut child = tokio::process::Command::new(&binary)
            .args(["--model", model.to_str().unwrap_or(""),
                   "--output_file", tmp.to_str().unwrap_or(""),
                   "--sentence_silence", "0.1"])
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()?;

        if let Some(mut stdin) = child.stdin.take() {
            use tokio::io::AsyncWriteExt;
            stdin.write_all(clean.as_bytes()).await?;
        }

        let status = child.wait().await?;
        eprintln!("[TTS] piper exited: {:?}", status.code());
        if !status.success() {
            return Err(anyhow::anyhow!("piper exited {:?}", status.code()));
        }

        let wav = tokio::fs::read(&tmp).await?;
        eprintln!("[TTS] wav size: {} bytes", wav.len());
        let _ = tokio::fs::remove_file(&tmp).await;
        let pcm = wav_to_f32(&wav);
        tokio::task::spawn_blocking(move || play_pcm_blocking(&pcm))
            .await
            .map_err(|e| anyhow::anyhow!("playback: {e}"))??;

        Ok(())
    }
}

fn find_piper() -> Result<std::path::PathBuf> {
    crate::find_sidecar("piper")
        .ok_or_else(|| anyhow::anyhow!(
            "piper binary not found in binaries/piper/ — run: npm run setup"
        ))
}

fn find_tts_model() -> Result<std::path::PathBuf> {
    // In dev: models/tts/ next to project root; in release: relative to exe
    let candidates = [
        crate::binaries_dir().parent()
            .map(|p| p.join("models").join("tts").join("en_US-lessac-medium.onnx"))
            .unwrap_or_default(),
        crate::binaries_dir()
            .parent().and_then(|p| p.parent())
            .map(|p| p.join("models").join("tts").join("en_US-lessac-medium.onnx"))
            .unwrap_or_default(),
    ];
    candidates.into_iter()
        .find(|p| p.exists())
        .ok_or_else(|| anyhow::anyhow!(
            "piper voice model not found — run: npm run setup"
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

fn wav_to_f32(wav: &[u8]) -> Vec<f32> {
    if wav.len() < 44 { return vec![]; }
    wav[44..].chunks_exact(2)
        .map(|b| i16::from_le_bytes([b[0], b[1]]) as f32 / 32768.0)
        .collect()
}

fn play_pcm_blocking(samples: &[f32]) -> Result<()> {
    if samples.is_empty() { return Ok(()); }
    let host   = cpal::default_host();
    let device = host.default_output_device()
        .ok_or_else(|| anyhow::anyhow!("no output device"))?;
    let config: cpal::StreamConfig = device.default_output_config()?.into();
    let samples  = Arc::new(samples.to_vec());
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
