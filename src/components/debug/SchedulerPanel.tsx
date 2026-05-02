import { invoke } from '@tauri-apps/api/core';
import { useState } from 'react';
import { DeferredMessage, SchedulerState } from '../../types';

interface Props { scheduler: SchedulerState; }

export function SchedulerPanel({ scheduler }: Props) {
  const [firing, setFiring] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  const fireNow = async (id: string) => {
    setFiring(id);
    setError(null);
    try {
      await invoke('fire_deferred_now', { id });
    } catch (e) {
      setError(String(e));
    } finally {
      setFiring(null);
    }
  };

  return (
    <div style={{ display: 'flex', flexDirection: 'column', gap: 8, fontSize: 12 }}>
      {error && <div style={{ color: 'var(--error)', fontSize: 11 }}>{error}</div>}

      {scheduler.pending.length === 0 ? (
        <div style={{ color: 'var(--text-muted)' }}>no pending messages</div>
      ) : (
        scheduler.pending.map(msg => (
          <PendingRow
            key={msg.id}
            msg={msg}
            onFire={fireNow}
            isFiring={firing === msg.id}
          />
        ))
      )}

      {scheduler.last_fired && (
        <div style={{ borderTop: '1px solid var(--border)', paddingTop: 6, color: 'var(--text-muted)', fontSize: 11 }}>
          last fired: <span style={{ color: 'var(--text)' }}>{scheduler.last_fired.message}</span>
        </div>
      )}
    </div>
  );
}

function PendingRow({ msg, onFire, isFiring }: {
  msg: DeferredMessage;
  onFire: (id: string) => void;
  isFiring: boolean;
}) {
  const remaining = Math.max(0, new Date(msg.fire_at).getTime() - Date.now());
  const mins = Math.floor(remaining / 60000);
  const secs = Math.floor((remaining % 60000) / 1000);

  return (
    <div style={{
      display: 'flex', alignItems: 'flex-start', gap: 8,
      padding: '6px 8px', background: 'var(--bg)',
      border: '1px solid var(--border)', borderRadius: 'var(--radius)',
    }}>
      <div style={{ flex: 1 }}>
        <div style={{ marginBottom: 2 }}>{msg.message}</div>
        <div style={{ color: 'var(--text-muted)', fontSize: 11 }}>
          trigger: {msg.trigger} · in {mins}m {secs}s
        </div>
      </div>
      <button
        onClick={() => onFire(msg.id)}
        disabled={isFiring}
        className="primary"
        style={{ fontSize: 10, padding: '2px 8px', flexShrink: 0 }}
        title="Fire this message immediately (dev shortcut)"
      >
        {isFiring ? '…' : 'fire now'}
      </button>
    </div>
  );
}
