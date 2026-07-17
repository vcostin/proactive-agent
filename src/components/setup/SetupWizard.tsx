import { open } from '@tauri-apps/plugin-dialog';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { useEffect, useState } from 'react';
import { BinariesStatus, DownloadProgress, SetupStatus, SystemDeps } from '../../types';
import { SystemRequirements } from './SystemRequirements';

interface Props {
  status: SetupStatus;
  /** True when reopened via Setup repair (memory/config intact). */
  repair?: boolean;
  onComplete: (modelPath: string) => void;
  /** Close repair and return to main UI without changing the chat model. */
  onClose?: () => void;
}

type Step = 'tools' | 'models' | 'chat';

function initialStep(status: SetupStatus): Step {
  // Gates come from derived SetupStatus (catalog verify + required_for_*).
  // Piper stays out of the Host completion bar.
  if (!status.binaries.llama_ready) return 'tools';
  if (!status.embed_model_ready || !status.stt_ready) return 'models';
  return 'chat';
}

export function SetupWizard({ status, repair = false, onComplete, onClose }: Props) {
  const [live, setLive]           = useState<SetupStatus>(status);
  const [step, setStep]           = useState<Step>(initialStep(status));
  const [deps, setDeps]           = useState<SystemDeps | null>(null);
  const [downloading, setDownloading] = useState(false);
  const [progress, setProgress]   = useState<Record<string, DownloadProgress>>({});
  const [error, setError]         = useState<string | null>(null);

  const refreshStatus = async () => {
    const fresh = await invoke<SetupStatus>('get_setup_status');
    setLive(fresh);
    return fresh;
  };

  useEffect(() => {
    // Setup repair re-checks prerequisites and artifact readiness on open.
    refreshStatus().catch(() => {});
    invoke<SystemDeps>('check_system_deps').then(d => setDeps(d)).catch(() => {});
  }, []);

  useEffect(() => {
    let unlisten: (() => void) | null = null;
    listen<DownloadProgress>('download_progress', e => {
      setProgress(prev => ({ ...prev, [e.payload.filename]: e.payload }));
    }).then(fn => { unlisten = fn; });
    return () => { unlisten?.(); };
  }, []);

  const handleDownloadTools = async () => {
    setDownloading(true);
    setError(null);
    try {
      await invoke('download_required_binaries');
      const fresh = await refreshStatus();
      if (fresh.binaries.llama_ready) setStep('models');
    } catch (e) {
      setError(String(e));
    } finally {
      setDownloading(false);
    }
  };

  const handleDownloadModels = async () => {
    setDownloading(true);
    setError(null);
    try {
      await invoke('download_required_models');
      await refreshStatus();
      setStep('chat');
    } catch (e) {
      setError(String(e));
    } finally {
      setDownloading(false);
    }
  };

  const handlePickModel = async () => {
    setError(null);
    try {
      const path = await open({
        title: 'Select your chat model',
        filters: [{ name: 'GGUF Model', extensions: ['gguf'] }],
        multiple: false,
      });
      if (path && typeof path === 'string') {
        await invoke('swap_model', { modelPath: path });
        onComplete(path);
      }
    } catch (e) {
      setError(String(e));
    }
  };

  const binaries = live.binaries;
  // Core tools done when llama is present; piper remains optional in the list.
  const toolsDone = binaries.llama_ready;
  // Host STT done uses derived stt_ready (encoder + decoder + vocab + ORT).
  const modelsDone = live.embed_model_ready && live.stt_ready;
  const sttReady = live.stt_ready;

  return (
    <div style={{
      height: '100%', display: 'flex', alignItems: 'center',
      justifyContent: 'center', background: 'var(--bg)',
    }}>
      <div style={{
        width: 500, background: 'var(--bg-panel)',
        border: '1px solid var(--border)', borderRadius: 10,
        overflow: 'hidden',
      }}>
        <div style={{ padding: '20px 24px 16px', borderBottom: '1px solid var(--border)' }}>
          <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'baseline' }}>
            <div style={{ fontSize: 16, fontWeight: 600, marginBottom: 4 }}>proactive-agent</div>
            {repair && onClose && (
              <button onClick={onClose} style={{ fontSize: 11, padding: '2px 10px' }}>
                back to app
              </button>
            )}
          </div>
          <div style={{ fontSize: 12, color: 'var(--text-muted)' }}>
            {repair ? 'Setup repair — re-check and restore app-managed artifacts' : 'Setup Wizard'}
          </div>
        </div>

        <div style={{
          display: 'flex', padding: '12px 24px',
          borderBottom: '1px solid var(--border)', gap: 8, alignItems: 'center',
        }}>
          <StepPill n={1} label="tools"   active={step === 'tools'}  done={toolsDone} />
          <Connector />
          <StepPill n={2} label="models"  active={step === 'models'} done={modelsDone} />
          <Connector />
          <StepPill n={3} label="chat model" active={step === 'chat'} done={!!live.chat_model} />
        </div>

        <div style={{ padding: '20px 24px' }}>
          {step === 'tools' && (
            <ToolsStep
              binaries={binaries}
              sttReady={sttReady}
              progress={progress}
              downloading={downloading}
              error={error}
              onDownload={handleDownloadTools}
              onSkip={() => setStep('models')}
            />
          )}
          {step === 'models' && (
            <ModelsStep
              status={live}
              sttReady={sttReady}
              deps={deps}
              onDepsChange={setDeps}
              progress={progress}
              downloading={downloading}
              error={error}
              onDownload={handleDownloadModels}
              onSkip={() => setStep('chat')}
            />
          )}
          {step === 'chat' && (
            <ChatModelStep
              error={error}
              onPick={handlePickModel}
              repair={repair}
              hasModel={!!live.chat_model}
              onClose={onClose}
            />
          )}
        </div>

        <div style={{
          padding: '10px 24px 16px',
          color: 'var(--text-muted)', fontSize: 11,
          borderTop: '1px solid var(--border)',
        }}>
          data stored in: {live.data_dir}
        </div>
      </div>
    </div>
  );
}

// ── Step 1: tools (llama-server required; piper optional) ────────────────────

function ToolsStep({ binaries, sttReady, progress, downloading, error, onDownload, onSkip }: {
  binaries: BinariesStatus;
  sttReady: boolean;
  progress: Record<string, DownloadProgress>;
  downloading: boolean;
  error: string | null;
  onDownload: () => void;
  onSkip: () => void;
}) {
  const tools = [
    {
      key: 'llama-server',
      label: 'llama-server',
      desc: 'LLM inference engine — required for the Core agent',
      size: '~90 MB',
      ready: binaries.llama_ready,
      optional: false,
    },
    {
      key: 'piper',
      label: 'Piper TTS',
      desc: 'Text-to-speech (optional — not required for Core agent / Host done bar)',
      size: '~5 MB',
      ready: binaries.piper_ready,
      optional: true,
    },
    {
      key: 'onnxruntime',
      label: 'ONNX Runtime',
      desc: binaries.ort_note,
      size: '~6 MB',
      ready: binaries.ort_ready,
      optional: true,
    },
  ];

  return (
    <div style={{ display: 'flex', flexDirection: 'column', gap: 12 }}>
      <p style={{ fontSize: 12, color: 'var(--text-muted)', margin: 0 }}>
        App-managed sidecars run locally. llama-server is required for chat;
        Piper TTS is optional. Host STT readiness is shown below.
      </p>

      <div style={{
        fontSize: 11, padding: '6px 10px',
        border: `1px solid ${sttReady ? 'var(--success)' : 'var(--border)'}`,
        borderRadius: 'var(--radius)', color: 'var(--text-muted)',
      }}>
        Host STT path: {sttReady ? 'ready (model + vocab + ONNX Runtime)' : 'not ready — repair via models step / Setup repair'}
      </div>

      {tools.map(t => {
        const p = progress[t.key] ?? progress[`${t.key}.zip`] ?? progress['onnxruntime.tgz'];
        const pct = p && p.total > 0 ? Math.round((p.downloaded / p.total) * 100) : 0;
        const borderColor = t.ready ? 'var(--success)' : 'var(--border)';

        return (
          <div key={t.key} style={{
            padding: '10px 12px', background: 'var(--bg)',
            border: `1px solid ${borderColor}`,
            borderRadius: 'var(--radius)',
          }}>
            <div style={{ display: 'flex', justifyContent: 'space-between', marginBottom: 4 }}>
              <span style={{ fontWeight: 500, fontSize: 12 }}>
                {t.label}{t.optional ? ' (optional)' : ''}
              </span>
              <span style={{ fontSize: 11, color: t.ready ? 'var(--success)' : 'var(--text-muted)' }}>
                {t.ready ? '✓ ready' : t.size}
              </span>
            </div>
            <div style={{ fontSize: 11, color: 'var(--text-muted)', marginBottom: p && !p.done ? 6 : 0 }}>
              {t.desc}
            </div>
            {p && !p.done && (
              <div>
                <div style={{ height: 3, background: '#222', borderRadius: 2, overflow: 'hidden' }}>
                  <div style={{ height: '100%', width: `${pct}%`, background: 'var(--accent)', transition: 'width 0.2s' }} />
                </div>
                <div style={{ fontSize: 10, color: 'var(--text-muted)', marginTop: 3 }}>
                  {fmtBytes(p.downloaded)} / {fmtBytes(p.total)} — {pct}%
                </div>
              </div>
            )}
          </div>
        );
      })}

      {error && <ErrorBox msg={error} />}

      <div style={{ display: 'flex', gap: 8, justifyContent: 'flex-end' }}>
        {binaries.llama_ready
          ? <button className="primary" onClick={onSkip} style={{ padding: '6px 20px' }}>next →</button>
          : <>
              <button onClick={onSkip} style={{ padding: '6px 14px', fontSize: 11, color: 'var(--text-muted)' }}>
                skip
              </button>
              <button className="primary" onClick={onDownload} disabled={downloading}
                style={{ padding: '6px 20px' }}>
                {downloading ? 'downloading…' : 'download tools'}
              </button>
            </>
        }
      </div>
    </div>
  );
}

// ── Step 2: required models ───────────────────────────────────────────────────

function ModelsStep({ status, sttReady, deps, onDepsChange, progress, downloading, error, onDownload, onSkip }: {
  status: SetupStatus;
  sttReady: boolean;
  deps: SystemDeps | null;
  onDepsChange: (d: SystemDeps) => void;
  progress: Record<string, DownloadProgress>;
  downloading: boolean;
  error: string | null;
  onDownload: () => void;
  onSkip: () => void;
}) {
  const models = [
    {
      filename: 'nomic-embed-text-v1.5.Q8_0.gguf',
      label: 'nomic-embed-text',
      desc: 'Vector memory — fixed, never swapped',
      size: '274 MB',
      ready: status.embed_model_ready,
    },
    {
      filename: 'encoder-model.int8.onnx',
      label: 'Parakeet TDT encoder',
      desc: 'Speech-to-text encoder (Host STT path)',
      size: '~620 MB',
      ready: status.stt_model_ready,
    },
    {
      filename: 'decoder_joint-model.int8.onnx',
      label: 'Parakeet TDT decoder',
      desc: 'Speech-to-text decoder-joint (Host STT path)',
      size: '~18 MB',
      ready: status.stt_model_ready,
    },
    {
      filename: 'vocab.txt',
      label: 'Parakeet vocabulary',
      desc: 'STT token vocabulary',
      size: '~90 KB',
      ready: status.stt_vocab_ready,
    },
  ];

  const allReady = models.every(m => m.ready);

  return (
    <div style={{ display: 'flex', flexDirection: 'column', gap: 12 }}>
      <SystemRequirements onChange={onDepsChange} />

      <p style={{ fontSize: 12, color: 'var(--text-muted)', margin: 0 }}>
        Embed + STT models are app-managed artifacts. Skip STT if you only need text chat;
        Core agent still works. Missing STT is repairable via Setup repair.
      </p>

      <div style={{
        fontSize: 11, padding: '6px 10px',
        border: `1px solid ${sttReady ? 'var(--success)' : 'var(--border)'}`,
        borderRadius: 'var(--radius)', color: 'var(--text-muted)',
      }}>
        Host STT: {sttReady ? 'ready' : 'incomplete — download STT models and ONNX Runtime (tools step)'}
      </div>

      {models.map(m => {
        const p = progress[m.filename];
        const pct = p && p.total > 0 ? Math.round((p.downloaded / p.total) * 100) : 0;
        return (
          <div key={m.filename} style={{
            padding: '10px 12px', background: 'var(--bg)',
            border: `1px solid ${m.ready ? 'var(--success)' : 'var(--border)'}`,
            borderRadius: 'var(--radius)',
          }}>
            <div style={{ display: 'flex', justifyContent: 'space-between', marginBottom: 4 }}>
              <span style={{ fontWeight: 500, fontSize: 12 }}>{m.label}</span>
              <span style={{ fontSize: 11, color: m.ready ? 'var(--success)' : 'var(--text-muted)' }}>
                {m.ready ? '✓ ready' : m.size}
              </span>
            </div>
            <div style={{ fontSize: 11, color: 'var(--text-muted)', marginBottom: p && !p.done ? 6 : 0 }}>
              {m.desc}
            </div>
            {p && !p.done && (
              <div>
                <div style={{ height: 3, background: '#222', borderRadius: 2, overflow: 'hidden' }}>
                  <div style={{ height: '100%', width: `${pct}%`, background: 'var(--accent)', transition: 'width 0.2s' }} />
                </div>
                <div style={{ fontSize: 10, color: 'var(--text-muted)', marginTop: 3 }}>
                  {fmtBytes(p.downloaded)} / {fmtBytes(p.total)} — {pct}%
                </div>
              </div>
            )}
          </div>
        );
      })}

      {error && <ErrorBox msg={error} />}

      {deps && !deps.llama_server_ok && (
        <div style={{
          fontSize: 11, padding: '6px 10px',
          background: 'rgba(224,85,85,0.1)',
          border: '1px solid var(--error)',
          borderRadius: 'var(--radius)', color: 'var(--error)',
        }}>
          llama-server won't run until system prerequisites are met — see guidance above.
        </div>
      )}

      <div style={{ display: 'flex', gap: 8, justifyContent: 'flex-end' }}>
        {allReady
          ? <button className="primary" onClick={onSkip} style={{ padding: '6px 20px' }}>next →</button>
          : <>
              <button onClick={onSkip} style={{ padding: '6px 14px', fontSize: 11, color: 'var(--text-muted)' }}>
                skip (voice deferred)
              </button>
              <button className="primary" onClick={onDownload} disabled={downloading}
                style={{ padding: '6px 20px' }}>
                {downloading ? 'downloading…' : 'download models'}
              </button>
            </>
        }
      </div>
    </div>
  );
}

// ── Step 3: chat model ────────────────────────────────────────────────────────

function ChatModelStep({ error, onPick, repair, hasModel, onClose }: {
  error: string | null;
  onPick: () => void;
  repair?: boolean;
  hasModel?: boolean;
  onClose?: () => void;
}) {
  return (
    <div style={{ display: 'flex', flexDirection: 'column', gap: 16 }}>
      <p style={{ fontSize: 12, color: 'var(--text-muted)', margin: 0 }}>
        Pick a <strong>.gguf</strong> chat model from anywhere on your disk.
        Changing the model does not wipe memory or other artifacts.
      </p>

      <div style={{ display: 'flex', flexDirection: 'column', gap: 1 }}>
        <div style={{ fontSize: 10, color: 'var(--text-muted)', marginBottom: 4, textTransform: 'uppercase', letterSpacing: '0.08em' }}>
          suggested models (download separately from HuggingFace)
        </div>
        {[
          ['Qwen2.5-14B-Instruct-Q8_0.gguf',      '~15 GB', 'excellent'],
          ['Llama-3.1-8B-Instruct-Q8_0.gguf',     '~9 GB',  'fast + solid'],
          ['Mistral-7B-Instruct-v0.3.Q8_0.gguf',  '~8 GB',  'fast'],
        ].map(([name, size, note]) => (
          <div key={name} style={{
            display: 'flex', justifyContent: 'space-between',
            fontSize: 11, padding: '4px 8px',
            background: 'var(--bg)', borderRadius: 4,
            borderLeft: '2px solid var(--border)',
          }}>
            <span style={{ color: 'var(--text-muted)', fontFamily: 'monospace' }}>{name}</span>
            <span style={{ color: 'var(--text-muted)', flexShrink: 0, marginLeft: 8 }}>{size} · {note}</span>
          </div>
        ))}
      </div>

      {error && <ErrorBox msg={error} />}

      <div style={{ display: 'flex', gap: 8, justifyContent: 'flex-end' }}>
        {repair && hasModel && onClose && (
          <button onClick={onClose} style={{ padding: '8px 16px', fontSize: 12 }}>
            keep current model
          </button>
        )}
        <button className="primary" onClick={onPick} style={{ padding: '8px 24px', fontSize: 13 }}>
          Browse for .gguf file…
        </button>
      </div>
    </div>
  );
}

// ── Shared helpers ────────────────────────────────────────────────────────────

function StepPill({ n, label, active, done }: { n: number; label: string; active: boolean; done: boolean }) {
  return (
    <div style={{ display: 'flex', flexDirection: 'column', alignItems: 'center', gap: 4, minWidth: 72 }}>
      <div style={{
        width: 24, height: 24, borderRadius: '50%', display: 'flex',
        alignItems: 'center', justifyContent: 'center', fontSize: 11,
        background: done ? 'var(--success)' : active ? 'var(--accent)' : 'var(--bg)',
        border: `1px solid ${done ? 'var(--success)' : active ? 'var(--accent)' : 'var(--border)'}`,
        color: done || active ? 'var(--bg)' : 'var(--text-muted)',
      }}>
        {done ? '✓' : n}
      </div>
      <span style={{ fontSize: 10, color: active ? 'var(--accent)' : 'var(--text-muted)', textAlign: 'center' }}>
        {label}
      </span>
    </div>
  );
}

function Connector() {
  return <div style={{ flex: 1, borderTop: '1px solid var(--border)', marginTop: 12, alignSelf: 'flex-start' }} />;
}

function ErrorBox({ msg }: { msg: string }) {
  return (
    <div style={{
      fontSize: 11, color: 'var(--error)', padding: '6px 10px',
      border: '1px solid var(--error)', borderRadius: 'var(--radius)',
    }}>
      {msg}
    </div>
  );
}

function fmtBytes(n: number) {
  if (n >= 1024 ** 3) return `${(n / 1024 ** 3).toFixed(1)} GB`;
  if (n >= 1024 ** 2) return `${(n / 1024 ** 2).toFixed(0)} MB`;
  return `${(n / 1024).toFixed(0)} KB`;
}
