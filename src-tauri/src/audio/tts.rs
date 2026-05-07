use anyhow::Result;

/// Text-to-speech using the platform's built-in synthesizer.
/// Windows: System.Speech (SAPI) via PowerShell — always installed, zero config.
/// macOS:   `say` command — always installed.
pub struct TtsClient;

impl TtsClient {
    pub fn new(_port: u16) -> Self { Self }

    /// Speak `text` synchronously. Call from spawn_blocking.
    pub async fn speak(&self, text: &str) -> Result<()> {
        // Clean markdown formatting so it doesn't get read aloud
        let clean = clean_for_speech(text);
        if clean.trim().is_empty() { return Ok(()); }

        #[cfg(target_os = "windows")]
        {
            speak_sapi(&clean).await?;
        }
        #[cfg(target_os = "macos")]
        {
            speak_say(&clean).await?;
        }
        #[cfg(not(any(target_os = "windows", target_os = "macos")))]
        {
            eprintln!("[TTS] platform not supported");
        }

        Ok(())
    }
}

/// Remove markdown that would be read aloud as noise.
fn clean_for_speech(text: &str) -> String {
    let mut s = text.to_string();
    // Remove code blocks entirely
    while let (Some(start), Some(end)) = (s.find("```"), s[s.find("```").unwrap_or(0) + 3..].find("```").map(|i| i + s.find("```").unwrap_or(0) + 6)) {
        s = format!("{}{}", &s[..start], &s[end..]);
    }
    // Remove inline code
    s = s.replace('`', "");
    // Remove bold/italic markers
    s = s.replace("**", "").replace("__", "").replace('*', "").replace('_', " ");
    // Remove URLs
    if let Ok(re) = regex::Regex::new(r"https?://\S+") {
        s = re.replace_all(&s, "link").to_string();
    }
    // Collapse whitespace
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(target_os = "windows")]
async fn speak_sapi(text: &str) -> Result<()> {
    // Escape single quotes for PowerShell string
    let escaped = text.replace('\'', "''");
    let script = format!(
        "Add-Type -AssemblyName System.Speech; \
         $s = New-Object System.Speech.Synthesis.SpeechSynthesizer; \
         $s.Rate = 1; \
         $s.Speak('{escaped}');",
    );
    let status = tokio::process::Command::new("powershell")
        .args(["-NoProfile", "-NonInteractive", "-Command", &script])
        .status()
        .await?;
    if !status.success() {
        return Err(anyhow::anyhow!("SAPI exit code: {:?}", status.code()));
    }
    Ok(())
}

#[cfg(target_os = "macos")]
async fn speak_say(text: &str) -> Result<()> {
    tokio::process::Command::new("say")
        .arg(text)
        .status()
        .await?;
    Ok(())
}
