use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    /// Absolute path to the currently loaded chat model (.gguf).
    /// Empty = no model selected yet (triggers the setup wizard).
    pub chat_model: String,

    /// Directory where auto-downloaded models are stored.
    pub models_dir: PathBuf,

    /// Directory where LanceDB memory is stored.
    pub db_path: PathBuf,

    pub llama_port: u16,
    pub embed_port: u16,
    /// Port for the Parakeet TDT STT sidecar (was whisper_port).
    pub stt_port: u16,

    /// None means cpal picks the system default.
    pub audio_device: Option<String>,

    /// Logical name passed to the embeddings API — never changed.
    pub embed_model: String,
    /// Filename of the nomic-embed-text GGUF inside models_dir.
    pub embed_model_file: String,

    pub persona_prompt: String,
    pub context_window_tokens: usize,
    pub top_k_episodic: usize,
    pub top_k_semantic: usize,
    pub recent_turns_window: usize,
    pub temperature: f32,
    pub top_p: f32,
}

impl AppConfig {
    pub fn with_data_dir(data_dir: PathBuf) -> Self {
        #[cfg(debug_assertions)]
        let (models_dir, db_path) = {
            let cwd = std::env::current_dir().unwrap_or_else(|_| data_dir.clone());
            let root = match cwd.file_name().and_then(|n| n.to_str()) {
                Some("src-tauri") => cwd.parent().unwrap_or(&cwd).to_path_buf(),
                _ => cwd,
            };
            (root.join("models"), root.join("data").join("memory"))
        };
        #[cfg(not(debug_assertions))]
        let (models_dir, db_path) = (data_dir.join("models"), data_dir.join("memory"));

        Self {
            chat_model: String::new(),
            models_dir,
            db_path,
            llama_port: 18080,
            embed_port: 18081,
            stt_port: 5092, // legacy config field; Host STT is in-process (no port)
            audio_device: None,
            embed_model: crate::constants::EMBED_MODEL_ALIAS.to_string(),
            embed_model_file: crate::constants::EMBED_MODEL_FILE.to_string(),
            persona_prompt: concat!(
                "You are a helpful, proactive assistant with persistent memory. ",
                "You remember facts about the user across conversations. ",
                "\n\nPROACTIVITY: When a topic is unresolved or would benefit from a follow-up, ",
                "append a <defer> tag AFTER your response (not inside it). Format exactly:\n",
                "<defer>{\"message\":\"Your follow-up message here\",\"after_minutes\":60,\"trigger\":\"reason\"}</defer>\n",
                "Use this sparingly — only for genuinely useful follow-ups like:\n",
                "- Checking on a task the user mentioned\n",
                "- Reminding about something time-sensitive\n",
                "- Following up on an unresolved question",
            ).to_string(),
            context_window_tokens: 4096,
            top_k_episodic: 5,
            top_k_semantic: 5,
            recent_turns_window: 10,
            temperature: 0.7,
            top_p: 0.95,
        }
    }

    pub fn load(config_path: &std::path::Path, data_dir: PathBuf) -> Self {
        if let Ok(text) = std::fs::read_to_string(config_path) {
            if let Ok(cfg) = serde_json::from_str::<AppConfig>(&text) {
                return cfg;
            }
        }
        Self::with_data_dir(data_dir)
    }

    pub fn save(&self, config_path: &std::path::Path) -> anyhow::Result<()> {
        if let Some(parent) = config_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(config_path, serde_json::to_string_pretty(self)?)?;
        Ok(())
    }

    pub fn is_ready(&self) -> bool {
        !self.chat_model.is_empty()
            && std::path::Path::new(&self.chat_model).exists()
    }

    pub fn embed_model_path(&self) -> PathBuf {
        self.models_dir.join(&self.embed_model_file)
    }

    /// Path where Parakeet TDT model files are expected.
    pub fn stt_model_dir() -> PathBuf {
        crate::binaries_dir().join(crate::constants::STT_MODEL_REL_DIR)
    }

    pub fn stt_model_ready() -> bool {
        crate::setup::status::stt_model_ready_in(&crate::binaries_dir())
            && crate::setup::status::stt_vocab_ready_in(&crate::binaries_dir())
    }

    /// App-managed ONNX Runtime library directory.
    pub fn ort_lib_dir() -> PathBuf {
        crate::binaries_dir().join(crate::constants::ORT_LIB_REL_DIR)
    }
}
