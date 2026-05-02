import { listen } from '@tauri-apps/api/event';
import { useEffect, useState } from 'react';
import { SystemDeps } from '../../types';
import { SystemRequirements } from './SystemRequirements';

/**
 * Persistent banner shown at the top of the app whenever system requirements
 * are not met — visible even after the setup wizard is complete.
 */
export function RequirementsBanner() {
  const [deps, setDeps] = useState<SystemDeps | null>(null);
  const [showDetails, setShowDetails] = useState(false);
  const [dismissed, setDismissed] = useState(false);

  useEffect(() => {
    let unlisten: (() => void) | null = null;
    listen<SystemDeps>('system_deps_checked', e => {
      setDeps(e.payload);
      // Auto-open details if something critical is broken
      if (!e.payload.llama_server_ok) setDismissed(false);
    }).then(fn => { unlisten = fn; });
    return () => { unlisten?.(); };
  }, []);

  // Nothing to show if requirements are met or dismissed or not yet checked
  if (!deps || deps.llama_server_ok || dismissed) return null;

  return (
    <>
      {/* Banner */}
      <div style={{
        display: 'flex', alignItems: 'center', gap: 10,
        padding: '7px 14px',
        background: 'rgba(224,85,85,0.12)',
        borderBottom: '1px solid var(--error)',
        flexShrink: 0,
        fontSize: 12,
      }}>
        <span style={{ color: 'var(--error)' }}>⚠</span>
        <span style={{ flex: 1, color: 'var(--text)' }}>
          <strong>llama-server can't start</strong>
          {' — '}{deps.llama_server_msg}
        </span>
        <button
          className="primary"
          onClick={() => setShowDetails(true)}
          style={{ fontSize: 11, padding: '2px 12px', borderColor: 'var(--error)', color: 'var(--error)' }}
        >
          Fix
        </button>
        <button
          onClick={() => setDismissed(true)}
          style={{ fontSize: 11, padding: '2px 8px', color: 'var(--text-muted)' }}
          title="Dismiss (will reappear on next launch)"
        >
          ✕
        </button>
      </div>

      {/* Fix modal */}
      {showDetails && (
        <div style={{
          position: 'fixed', inset: 0,
          background: 'rgba(0,0,0,0.75)',
          display: 'flex', alignItems: 'center', justifyContent: 'center',
          zIndex: 200,
        }}>
          <div style={{
            background: 'var(--bg-panel)',
            border: '1px solid var(--border)',
            borderRadius: 10,
            width: 480,
            overflow: 'hidden',
          }}>
            <div style={{
              display: 'flex', alignItems: 'center',
              padding: '12px 16px', borderBottom: '1px solid var(--border)',
            }}>
              <span style={{ flex: 1, fontWeight: 600 }}>System Requirements</span>
              <button
                onClick={() => setShowDetails(false)}
                style={{ fontSize: 11, padding: '2px 8px' }}
              >
                ✕ close
              </button>
            </div>
            <div style={{ padding: 16 }}>
              <SystemRequirements
                onChange={updated => {
                  setDeps(updated);
                  if (updated.llama_server_ok) {
                    setTimeout(() => setShowDetails(false), 1200);
                  }
                }}
              />
              <p style={{ fontSize: 11, color: 'var(--text-muted)', marginTop: 12 }}>
                After installing, restart the app to reload all sidecar processes.
              </p>
            </div>
          </div>
        </div>
      )}
    </>
  );
}
