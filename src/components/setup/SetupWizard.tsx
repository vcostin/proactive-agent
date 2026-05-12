import { open } from '@tauri-apps/plugin-dialog';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { useEffect, useState } from 'react';
import { BinariesStatus, DownloadProgress, SetupStatus, SystemDeps } from '../../types';
import { SystemRequirements } from './SystemRequirements';

interface Props {
  status: SetupStatus;
  onComplete: (modelPath: string) => void;
}

type Step = 'tools' | 'models' | 'chat';

function initialStep(status: SetupStatus): Step {
  if (!status.binaries.llama_ready || !status.binaries.piper_ready) return 'tools';
  if (!status.embed_model_ready || !status.stt_model_ready) return 'models';
  return 'chat';
}

export function SetupWizard({ status, onComplete }: Props) {
  const [step, setStep]           = useState<Step>(initialStep(status));
  const [binaries, setBinaries]   = useState<BinariesStatus>(status.binaries);
  const [deps, setDeps]           = useState<SystemDeps | null>(null);
  const [downloading, setDownloading] = useState(false);
  const [progress, setProgress]   = useState<Record<string, DownloadProgress>>({});
  const [error, setError]         = useState<string | null>(null);

  useEffect(() => {
    let unlisten: (() => void) | null = null;
    listen<DownloadProgress>('download_progress', e => {
      setProgress(prev => ({ ...prev, [e.payload.filename]: e.payload }));
    }).then(fn => { unlisten = fn; });
    return () => { unlisten?.(); };
  }, []);

  const handleDownloadTools = async () => {
    setDownloading(true);
    setProgress({});  // reset so stale done:true doesn't hide new bars
    setError(null);
    try {
      await invoke('download_required_binaries');
      const fresh = await invoke<BinariesStatus>('check_binaries_ready');
      setBinaries(fresh);
      if (fresh.llama_ready && fresh.piper_ready) {
        // Start llama-server and embed server now — no app restart needed
        await invoke('start_sidecars').catch(() => {});
        setStep('models');
      }
    } catch (e) {
      setError(String(e));
    } finally {
      setDownloading(false);
    }
  };

  const handleDownloadModels = async () => {
    setDownloading(true);
    setProgress({});  // reset so stale done:true doesn't hide new bars
    setError(null);
    try {
      await invoke('download_required_models');
      // Ensure sidecars are running before advancing
      await invoke('start_sidecars').catch(() => {});
      // Initialise ort STT session — show error if it fails so the user knows
      try {
        await invoke('init_stt_client');
      } catch (e) {
        // Non-fatal: user can still chat without voice, but show what happened
        setError(`Voice input unavailable: ${e}. You can continue without it.`);
      }
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

  const toolsDone = binaries.llama_ready && binaries.piper_ready;
  const modelsDone = status.embed_model_ready && status.stt_model_ready;

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
        {/* Header */}
        <div style={{ padding: '20px 24px 16px', borderBottom: '1px solid var(--border)' }}>
          <div style={{ fontSize: 16, fontWeight: 600, marginBottom: 4 }}>proactive-agent</div>
          <div style={{ fontSize: 12, color: 'var(--text-muted)' }}>first-run setup</div>
        </div>

        {/* Step indicator */}
        <div style={{
          display: 'flex', padding: '12px 24px',
          borderBottom: '1px solid var(--border)', gap: 8, alignItems: 'center',
        }}>
          <StepPill n={1} label="tools"   active={step === 'tools'}  done={toolsDone} />
          <Connector />
          <StepPill n={2} label="models"  active={step === 'models'} done={modelsDone} />
          <Connector />
          <StepPill n={3} label="chat model" active={step === 'chat'} done={!!status.chat_model} />
        </div>

        {/* Content */}
        <div style={{ padding: '20px 24px' }}>
          {step === 'tools' && (
            <ToolsStep
              binaries={binaries}
              progress={progress}
              downloading={downloading}
              error={error}
              onDownload={handleDownloadTools}
              onSkip={() => setStep('models')}
            />
          )}
          {step === 'models' && (
            <ModelsStep
              status={status}
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
            <ChatModelStep error={error} onPick={handlePickModel} />
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

// ── Step 1: tools (llama-server + piper) ─────────────────────────────────────

function ToolsStep({ binaries, progress, downloading, error, onDownload, onSkip }: {
  binaries: BinariesStatus;
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
      desc: 'LLM inference engine — CPU build + Vulkan GPU DLLs',
      size: '~90 MB',
      ready: binaries.llama_ready,
    },
    {
      key: 'piper',
      label: 'Piper TTS',
      desc: 'Text-to-speech + onnxruntime.dll (also used by STT)',
      size: '~5 MB',
      ready: binaries.piper_ready,
    },
    // Parakeet server binary removed — STT now runs in-process via ort.
    // The ONNX model files are downloaded in Step 2 (models).
  ];

  const allAutoReady = tools.every(t => t.ready);

  return (
    <div style={{ display: 'flex', flexDirection: 'column', gap: 12 }}>
      <p style={{ fontSize: 12, color: 'var(--text-muted)', margin: 0 }}>
        The inference engine and voice tools run locally on your machine.
        They download once from GitHub releases.
      </p>

      {tools.map(t => {
        // Key matches what binary_store.rs emits: "llama-server" or "piper"
        const p = progress[t.key];
        const pct = p && p.total > 0 ? Math.round((p.downloaded / p.total) * 100) : 0;
        const borderColor = p && !p.done ? 'var(--accent)' : t.ready ? 'var(--success)' : 'var(--border)';

        return (
          <div key={t.key} style={{
            padding: '10px 12px', background: 'var(--bg)',
            border: `1px solid ${borderColor}`,
            borderRadius: 'var(--radius)',
          }}>
            <div style={{ display: 'flex', justifyContent: 'space-between', marginBottom: 4 }}>
              <span style={{ fontWeight: 500, fontSize: 12 }}>{t.label}</span>
              <span style={{ fontSize: 11, color: t.ready ? 'var(--success)' : 'var(--text-muted)' }}>
                {t.ready ? '✓ ready' : t.size}
              </span>
            </div>
            <div style={{ fontSize: 11, color: 'var(--text-muted)', marginBottom: (p && !p.done) || (downloading && !t.ready && (!p || p.done)) ? 6 : 0 }}>
              {t.desc}
            </div>
            {downloading && !t.ready && (!p || p.done) && (
              <ProgressBar downloaded={0} total={0} pct={0} />
            )}
            {p && !p.done && <ProgressBar downloaded={p.downloaded} total={p.total} pct={pct} />}
          </div>
        );
      })}

      {error && <ErrorBox msg={error} />}

      <div style={{ display: 'flex', gap: 8, justifyContent: 'flex-end' }}>
        {allAutoReady
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
      filename: 'parakeet-tdt-0.6b-v3.onnx',
      label: 'Parakeet TDT 0.6B',
      desc: 'Speech-to-text ONNX model',
      size: '~600 MB',
      ready: status.stt_model_ready,
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
            <div style={{ fontSize: 11, color: 'var(--text-muted)', marginBottom: (p && !p.done) || (downloading && !m.ready && (!p || p.done)) ? 6 : 0 }}>
              {m.desc}
            </div>
            {/* Show indeterminate bar immediately on download start — before first chunk arrives */}
            {downloading && !m.ready && (!p || p.done) && (
              <ProgressBar downloaded={0} total={0} pct={0} />
            )}
            {p && !p.done && <ProgressBar downloaded={p.downloaded} total={p.total} pct={pct} />}
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
          ⚠ llama-server won't run until system requirements are met.
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
                {downloading ? 'downloading…' : 'download (~874 MB)'}
              </button>
            </>
        }
      </div>
    </div>
  );
}

// ── Step 3: chat model ────────────────────────────────────────────────────────

function ChatModelStep({ error, onPick }: { error: string | null; onPick: () => void }) {
  return (
    <div style={{ display: 'flex', flexDirection: 'column', gap: 16 }}>
      <p style={{ fontSize: 12, color: 'var(--text-muted)', margin: 0 }}>
        Pick a <strong>.gguf</strong> chat model from anywhere on your disk.
        For your hardware (16 GB VRAM) these are good fits:
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

      <div style={{ display: 'flex', justifyContent: 'flex-end' }}>
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

function ProgressBar({ downloaded, total, pct }: { downloaded: number; total: number; pct: number }) {
  const indeterminate = total === 0;
  return (
    <div style={{ marginTop: 8 }}>
      {/* Track */}
      <div style={{
        height: 6, background: 'rgba(255,255,255,0.08)',
        borderRadius: 3, overflow: 'hidden',
      }}>
        <div style={{
          height: '100%',
          width: indeterminate ? '40%' : `${pct}%`,
          background: 'var(--accent)',
          borderRadius: 3,
          transition: indeterminate ? 'none' : 'width 0.3s ease',
          animation: indeterminate ? 'progress-slide 1.4s ease-in-out infinite' : 'none',
        }} />
      </div>
      {/* Label */}
      <div style={{
        display: 'flex', justifyContent: 'space-between',
        fontSize: 10, color: 'var(--text-muted)', marginTop: 4,
      }}>
        <span>{fmtBytes(downloaded)}{total > 0 ? ` / ${fmtBytes(total)}` : ''}</span>
        {total > 0 && <span style={{ color: 'var(--accent)', fontWeight: 600 }}>{pct}%</span>}
      </div>
    </div>
  );
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
