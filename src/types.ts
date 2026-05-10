// Mirror types for the Rust structs exposed via Tauri commands.

export interface BinariesStatus {
  llama_ready: boolean;
  piper_ready: boolean;
  /** Always false until manually provided — no public release URL */
  parakeet_ready: boolean;
  parakeet_note: string;
}

export interface SetupStatus {
  ready: boolean;
  chat_model: string;
  embed_model_ready: boolean;
  /** Parakeet TDT ONNX model files present */
  stt_model_ready: boolean;
  data_dir: string;
  binaries: BinariesStatus;
}

export interface DownloadProgress {
  filename: string;
  downloaded: number;
  total: number;
  done: boolean;
}

export interface SystemDeps {
  vcredist_ok: boolean;
  vulkan_ok: boolean;
  llama_server_ok: boolean;
  llama_server_msg: string;
}


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

// ── Debug / monitor types ─────────────────────────────────────────────────────

export interface SidecarHealthEvent {
  name: string;
  alive: boolean;
  port: number;
  latency_ms: number;
  status_code: number;
}

export interface MemoryStats {
  episodic_count: number;
  semantic_count: number;
  last_write: string | null;
  last_distillation: string | null;
  last_embed_latency_ms: number;
}

export interface AudioState {
  device_name: string;
  sample_rate: number;
  channels: number;
  vad_state: 'Silent' | 'Active';
  energy_level: number;
  last_stt_result: string | null;
  last_stt_latency_ms: number;
  last_tts_latency_ms: number;
  tts_buffer_fill_pct: number;
}

export interface SchedulerState {
  pending: DeferredMessage[];
  last_fired: DeferredMessage | null;
}

export interface SystemStatus {
  sidecars: Record<string, SidecarHealthEvent>;
  active_model: ModelInfo | null;
  memory: MemoryStats;
  audio: AudioState;
  scheduler: SchedulerState;
}

export interface ContextBlock {
  label: string;
  content: string;
  token_count: number;
}

export interface Turn {
  role: string;
  content: string;
}

export interface AssembledContext {
  persona: ContextBlock;
  semantic: ContextBlock;
  episodic: ContextBlock;
  recent_turns: Turn[];
  user_input: string;
  assembled_at: string;
}

export interface DebugEvent {
  timestamp: string;
  component: string;
  message: string;
}
