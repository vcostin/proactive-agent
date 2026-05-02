import { open } from '@tauri-apps/plugin-dialog';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { useEffect, useState } from 'react';
import { DownloadProgress, SetupStatus, SystemDeps } from '../../types';
import { SystemRequirements } from './SystemRequirements';

interface Props {
  status: SetupStatus;
  onComplete: (modelPath: string) => void;
}

type Step = 'models' | 'chat';

export function SetupWizard({ status, onComplete }: Props) {
  const [step, setStep] = useState<Step>(!status.embed_model_ready || !status.whisper_model_ready ? 'models' : 'chat');
  const [deps, setDeps] = useState<SystemDeps | null>(null);
  const [downloading, setDownloading] = useState(false);
  const [progress, setProgress] = useState<Record<string, DownloadProgress>>({});
  const [error, setError] = useState<string | null>(null);

  // Subscribe to download progress events
  useEffect(() => {
    let unlisten: (() => void) | null = null;
    listen<DownloadProgress>('download_progress', e => {
      setProgress(prev => ({ ...prev, [e.payload.filename]: e.payload }));
    }).then(fn => { unlisten = fn; });
    return () => { unlisten?.(); };
  }, []);

  const handleDownload = async () => {
    setDownloading(true);
    setError(null);
    try {
      await invoke('download_required_models');
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

  return (
    <div style={{
      height: '100%', display: 'flex', alignItems: 'center',
      justifyContent: 'center', background: 'var(--bg)',
    }}>
      <div style={{
        width: 480, background: 'var(--bg-panel)',
        border: '1px solid var(--border)', borderRadius: 10,
        overflow: 'hidden',
      }}>
        {/* Header */}
        <div style={{
          padding: '20px 24px 16px',
          borderBottom: '1px solid var(--border)',
        }}>
          <div style={{ fontSize: 16, fontWeight: 600, marginBottom: 4 }}>
            proactive-agent
          </div>
          <div style={{ fontSize: 12, color: 'var(--text-muted)' }}>
            first-run setup — takes about a minute
          </div>
        </div>

        {/* Step indicator */}
        <div style={{
          display: 'flex', padding: '12px 24px',
          borderBottom: '1px solid var(--border)', gap: 8,
        }}>
          <StepPill n={1} label="required models" active={step === 'models'}
            done={status.embed_model_ready && status.whisper_model_ready} />
          <div style={{ flex: 1, borderTop: '1px solid var(--border)', marginTop: 12, alignSelf: 'flex-start' }} />
          <StepPill n={2} label="chat model" active={step === 'chat'}
            done={!!status.chat_model} />
        </div>

        {/* Content */}
        <div style={{ padding: '20px 24px' }}>
          {step === 'models' && (
            <ModelsStep
              status={status}
              deps={deps}
              onDepsChange={setDeps}
              progress={progress}
              downloading={downloading}
              error={error}
              onDownload={handleDownload}
              onSkip={() => setStep('chat')}
            />
          )}
          {step === 'chat' && (
            <ChatModelStep
              error={error}
              onPick={handlePickModel}
            />
          )}
        </div>

        <div style={{
          padding: '10px 24px 16px',
          color: 'var(--text-muted)', fontSize: 11,
          borderTop: '1px solid var(--border)',
        }}>
          data stored in: {status.data_dir}
        </div>
      </div>
    </div>
  );
}

// ── Step 1: required models ───────────────────────────────────────────────────

function ModelsStep({ status, deps, onDepsChange, progress, downloading, error, onDownload, onSkip }: {
  status: SetupStatus;
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
      filename: 'ggml-base.en.bin',
      label: 'Whisper base (English)',
      desc: 'Speech-to-text',
      size: '142 MB',
      ready: status.whisper_model_ready,
    },
  ];

  const allReady = models.every(m => m.ready);

  return (
    <div style={{ display: 'flex', flexDirection: 'column', gap: 12 }}>
      <SystemRequirements onChange={onDepsChange} />

      <p style={{ fontSize: 12, color: 'var(--text-muted)', margin: 0 }}>
        Two small models are required for vector memory and voice input.
        They download once and live in your app data folder.
      </p>

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

      {error && (
        <div style={{ fontSize: 11, color: 'var(--error)', padding: '6px 10px', border: '1px solid var(--error)', borderRadius: 'var(--radius)' }}>
          {error}
        </div>
      )}

      {/* Warn if deps not met, but don't block — user can proceed and fix later */}
      {deps && !deps.llama_server_ok && (
        <div style={{
          fontSize: 11, padding: '6px 10px',
          background: 'rgba(224,85,85,0.1)',
          border: '1px solid var(--error)',
          borderRadius: 'var(--radius)',
          color: 'var(--error)',
        }}>
          ⚠ llama-server won't run until system requirements are met.
          Fix the issues above, then continue.
        </div>
      )}

      <div style={{ display: 'flex', gap: 8, justifyContent: 'flex-end' }}>
        {allReady
          ? <button className="primary" onClick={onSkip} style={{ padding: '6px 20px' }}>next →</button>
          : <>
              <button onClick={onSkip} style={{ padding: '6px 14px', fontSize: 11, color: 'var(--text-muted)' }}>
                skip (voice disabled)
              </button>
              <button className="primary" onClick={onDownload} disabled={downloading}
                style={{ padding: '6px 20px' }}>
                {downloading ? 'downloading…' : 'download (~416 MB)'}
              </button>
            </>
        }
      </div>
    </div>
  );
}

// ── Step 2: chat model ────────────────────────────────────────────────────────

function ChatModelStep({ error, onPick }: { error: string | null; onPick: () => void }) {
  return (
    <div style={{ display: 'flex', flexDirection: 'column', gap: 16 }}>
      <p style={{ fontSize: 12, color: 'var(--text-muted)', margin: 0 }}>
        Pick a <strong>.gguf</strong> chat model from anywhere on your disk.
        For your hardware (16 GB VRAM) these are good fits — download one first,
        then click Browse.
      </p>

      {/* Suggestions — informational only, not clickable */}
      <div style={{ display: 'flex', flexDirection: 'column', gap: 1 }}>
        <div style={{ fontSize: 10, color: 'var(--text-muted)', marginBottom: 4, textTransform: 'uppercase', letterSpacing: '0.08em' }}>
          suggested models (download separately)
        </div>
        {[
          ['Qwen2.5-14B-Instruct-Q8_0.gguf', '~15 GB', 'excellent'],
          ['Llama-3.1-8B-Instruct-Q8_0.gguf', '~9 GB', 'fast + solid'],
          ['Mistral-7B-Instruct-v0.3.Q8_0.gguf', '~8 GB', 'fast'],
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
        <div style={{ fontSize: 11, color: 'var(--text-muted)', marginTop: 6 }}>
          Get them at{' '}
          <span style={{ color: 'var(--accent)' }}>huggingface.co/models?library=gguf</span>
          {' '}or{' '}
          <span style={{ color: 'var(--accent)' }}>lmstudio.ai/models</span>
        </div>
      </div>

      {error && (
        <div style={{ fontSize: 11, color: 'var(--error)', padding: '6px 10px', border: '1px solid var(--error)', borderRadius: 'var(--radius)' }}>
          {error}
        </div>
      )}

      <div style={{ display: 'flex', justifyContent: 'flex-end' }}>
        <button className="primary" onClick={onPick} style={{ padding: '8px 24px', fontSize: 13 }}>
          Browse for .gguf file…
        </button>
      </div>
    </div>
  );
}

// ── Helpers ───────────────────────────────────────────────────────────────────

function StepPill({ n, label, active, done }: { n: number; label: string; active: boolean; done: boolean }) {
  return (
    <div style={{ display: 'flex', flexDirection: 'column', alignItems: 'center', gap: 4, minWidth: 80 }}>
      <div style={{
        width: 24, height: 24, borderRadius: '50%', display: 'flex',
        alignItems: 'center', justifyContent: 'center', fontSize: 11,
        background: done ? 'var(--success)' : active ? 'var(--accent)' : 'var(--bg)',
        border: `1px solid ${done ? 'var(--success)' : active ? 'var(--accent)' : 'var(--border)'}`,
        color: done || active ? 'var(--bg)' : 'var(--text-muted)',
      }}>
        {done ? '✓' : n}
      </div>
      <span style={{ fontSize: 10, color: active ? 'var(--accent)' : 'var(--text-muted)' }}>{label}</span>
    </div>
  );
}

function fmtBytes(n: number) {
  if (n >= 1024 ** 3) return `${(n / 1024 ** 3).toFixed(1)} GB`;
  if (n >= 1024 ** 2) return `${(n / 1024 ** 2).toFixed(0)} MB`;
  return `${(n / 1024).toFixed(0)} KB`;
}
