import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { useEffect, useState } from 'react';
import { DownloadProgress, SystemDeps } from '../../types';

interface Props {
  /** Called whenever requirements are re-checked so the wizard can gate progress */
  onChange?: (deps: SystemDeps) => void;
}

type CheckState = 'checking' | 'done' | 'error';

export function SystemRequirements({ onChange }: Props) {
  const [state, setState] = useState<CheckState>('checking');
  const [deps, setDeps] = useState<SystemDeps | null>(null);
  const [installing, setInstalling] = useState(false);
  const [installProgress, setInstallProgress] = useState(0);
  const [installDone, setInstallDone] = useState(false);
  const [installError, setInstallError] = useState<string | null>(null);

  const check = async () => {
    setState('checking');
    try {
      const result = await invoke<SystemDeps>('check_system_deps');
      setDeps(result);
      setState('done');
      onChange?.(result);
    } catch (e) {
      setState('error');
    }
  };

  useEffect(() => { check(); }, []);

  // Listen for VCRedist download progress
  useEffect(() => {
    let unlisten: (() => void) | null = null;
    listen<DownloadProgress>('download_progress', e => {
      if (e.payload.filename === 'vc_redist.x64.exe') {
        if (e.payload.total > 0) {
          setInstallProgress(Math.round(e.payload.downloaded / e.payload.total * 100));
        }
      }
    }).then(fn => { unlisten = fn; });
    return () => { unlisten?.(); };
  }, []);

  const handleInstallVCRedist = async () => {
    setInstalling(true);
    setInstallError(null);
    setInstallProgress(0);
    setInstallDone(false);
    try {
      await invoke('install_vcredist');
      setInstallDone(true);
      // Re-check after install
      await check();
    } catch (e) {
      setInstallError(String(e));
    } finally {
      setInstalling(false);
    }
  };

  return (
    <div style={{
      background: 'var(--bg)',
      border: '1px solid var(--border)',
      borderRadius: 'var(--radius)',
      overflow: 'hidden',
      marginBottom: 4,
    }}>
      <div style={{
        padding: '6px 12px',
        fontSize: 10,
        color: 'var(--text-muted)',
        textTransform: 'uppercase',
        letterSpacing: '0.08em',
        borderBottom: '1px solid var(--border)',
        display: 'flex',
        alignItems: 'center',
        gap: 8,
      }}>
        <span>system requirements</span>
        {state === 'checking' && <Spinner />}
        {state === 'done' && (
          <button
            onClick={check}
            style={{ fontSize: 10, padding: '0 6px', marginLeft: 'auto' }}
          >
            ↻ re-check
          </button>
        )}
      </div>

      <div style={{ padding: '8px 12px', display: 'flex', flexDirection: 'column', gap: 8 }}>

        {state === 'checking' && (
          <div style={{ fontSize: 12, color: 'var(--text-muted)', padding: '4px 0' }}>
            Checking…
          </div>
        )}

        {state === 'done' && deps && (
          <>
            {/* Visual C++ Runtime */}
            {/* Show install/update whenever llama-server is failing — VCRedist may
                be installed but outdated (missing newer exported functions). */}
            <Row
              ok={deps.vcredist_ok && deps.llama_server_ok}
              label="Visual C++ Runtime 2022"
              okNote="installed"
              failNote={deps.vcredist_ok
                ? 'installed but outdated — needs update'
                : 'not installed — required for llama-server'}
              action={!deps.llama_server_ok && !installing && !installDone ? (
                <button
                  className="primary"
                  onClick={handleInstallVCRedist}
                  style={{ fontSize: 11, padding: '2px 10px' }}
                >
                  {deps.vcredist_ok ? 'update (~7 MB)' : 'install (~7 MB)'}
                </button>
              ) : undefined}
            />

            {/* Install progress */}
            {installing && (
              <div style={{ marginTop: -4 }}>
                <div style={{ height: 3, background: '#222', borderRadius: 2, overflow: 'hidden' }}>
                  <div style={{
                    height: '100%',
                    width: `${installProgress}%`,
                    background: 'var(--accent)',
                    transition: 'width 0.2s',
                  }} />
                </div>
                <div style={{ fontSize: 10, color: 'var(--text-muted)', marginTop: 3 }}>
                  {installProgress < 100
                    ? `downloading… ${installProgress}%`
                    : 'installing silently…'}
                </div>
              </div>
            )}
            {installDone && (
              <div style={{ fontSize: 11, color: 'var(--success)', marginTop: -4 }}>
                ✓ installed — re-checking…
              </div>
            )}
            {installError && (
              <div style={{ fontSize: 11, color: 'var(--error)', marginTop: -4 }}>
                {installError}
              </div>
            )}

            {/* Vulkan */}
            <Row
              ok={deps.vulkan_ok}
              label="Vulkan Runtime"
              okNote="detected"
              failNote="not found — update your GPU drivers (AMD/Nvidia)"
            />

            {/* llama-server test */}
            <Row
              ok={deps.llama_server_ok}
              label="llama-server binary"
              okNote={deps.llama_server_msg}
              failNote={deps.llama_server_msg}
              action={!deps.llama_server_ok ? (
                <button
                  onClick={async () => { await invoke('open_llama_diagnostic'); }}
                  style={{ fontSize: 10, padding: '1px 8px', flexShrink: 0 }}
                  title="Open a terminal window — Windows will show the exact missing DLL name"
                >
                  show error →
                </button>
              ) : undefined}
            />
          </>
        )}

        {state === 'error' && (
          <div style={{ fontSize: 12, color: 'var(--error)', display: 'flex', gap: 8, alignItems: 'center' }}>
            <span>Check failed</span>
            <button onClick={check} style={{ fontSize: 11 }}>retry</button>
          </div>
        )}
      </div>
    </div>
  );
}

function Row({ ok, label, okNote, failNote, action }: {
  ok: boolean;
  label: string;
  okNote: string;
  failNote: string;
  action?: React.ReactNode;
}) {
  return (
    <div style={{ display: 'flex', alignItems: 'center', gap: 8, fontSize: 12 }}>
      <span style={{
        width: 14, height: 14, borderRadius: '50%', flexShrink: 0,
        background: ok ? 'var(--success)' : 'var(--error)',
        boxShadow: ok ? '0 0 4px var(--success)' : '0 0 4px var(--error)',
      }} />
      <span style={{ flex: 1 }}>
        <span style={{ color: 'var(--text)' }}>{label}</span>
        {' '}
        <span style={{ fontSize: 11, color: ok ? 'var(--text-muted)' : 'var(--error)' }}>
          {ok ? okNote : failNote}
        </span>
      </span>
      {action}
    </div>
  );
}

function Spinner() {
  return (
    <span style={{
      display: 'inline-block',
      width: 10, height: 10,
      border: '1px solid var(--border)',
      borderTop: '1px solid var(--accent)',
      borderRadius: '50%',
      animation: 'spin 0.8s linear infinite',
    }} />
  );
}
