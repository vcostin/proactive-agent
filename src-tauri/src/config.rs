use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    pub models_dir: PathBuf,
    pub db_path: PathBuf,
    pub llama_port: u16,
    pub embed_port: u16,
    pub whisper_port: u16,
    pub kokoro_port: u16,
    /// None means cpal picks the system default
    pub audio_device: Option<String>,
    pub chat_model: String,
    /// Fixed to nomic-embed-text — never changed at runtime
    pub embed_model: String,
    /// Filename of the nomic-embed-text GGUF in models_dir
    pub embed_model_file: String,
    /// Filename of the Whisper GGUF/bin in models_dir
    pub whisper_model_file: String,
    pub persona_prompt: String,
    pub context_window_tokens: usize,
    pub top_k_episodic: usize,
    pub top_k_semantic: usize,
    pub recent_turns_window: usize,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            models_dir: PathBuf::from("models"),
            db_path: PathBuf::from("data/memory"),
            llama_port: 8080,
            embed_port: 8081,
            whisper_port: 8082,
            kokoro_port: 8083,
            audio_device: None,
            chat_model: String::new(),
            embed_model: "nomic-embed-text".to_string(),
            embed_model_file: "nomic-embed-text-v1.5.Q8_0.gguf".to_string(),
            whisper_model_file: "ggml-base.en.bin".to_string(),
            persona_prompt: concat!(
                "You are a helpful, proactive assistant with persistent memory. ",
                "You may schedule follow-up messages by emitting a <defer> tag at the end of ",
                "your response: <defer>{\"message\":\"...\",\"after_minutes\":N,\"trigger\":\"...\"}</defer>",
            )
            .to_string(),
            context_window_tokens: 4096,
            top_k_episodic: 5,
            top_k_semantic: 5,
            recent_turns_window: 10,
        }
    }
}
