// Mirror types for the Rust structs exposed via Tauri commands.

export interface ModelInfo {
  filename: string;
  quant_type: string;
  param_count: string;
  file_size_bytes: number;
  last_modified: string;
}

export interface ChatMessage {
  id: string;
  role: 'user' | 'assistant' | 'proactive';
  content: string;
  timestamp: string;
}

export interface DeferredMessage {
  id: string;
  message: string;
  trigger: string;
  fire_at: string;
}
