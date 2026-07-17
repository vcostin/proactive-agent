import { invoke } from '@tauri-apps/api/core';
import { useState } from 'react';
import { DeferredMessage, SchedulerState } from '../../types';

function TestDeferButton() {
  const [busy, setBusy] = useState(false);
  const [result, setResult] = useState('');
  return (
    <div style={{ marginTop: 8, paddingTop: 8, borderTop: '1px solid var(--border)' }}>
      <div style={{ fontSize: 10, color: 'var(--text-muted)', marginBottom: 4 }}>
        Test proactivity pipeline
      </div>
      <div style={{ display: 'flex', gap: 6 }}>
        <button
          onClick={async () => {
            setBusy(true);
            try {
              const r = await invoke<string>('test_defer', {
                message: 'This is a test proactive message from the scheduler.',
                afterMinutes: 0,
              });
              setResult(r);
            } catch (e) { setResult(String(e)); }
            finally { setBusy(false); }
          }}
          disabled={busy}
          className="primary"
          style={{ fontSize: 10, padding: '2px 10px' }}
        >
          {busy ? '…' : 'fire test defer now'}
        </button>
      </div>
      {result && <div style={{ fontSize: 10, color: 'var(--text-muted)', marginTop: 4 }}>{result}</div>}
    </div>
  );
}

interface Props { scheduler: SchedulerState; }

export function SchedulerPanel({ scheduler }: Props) {
  const [busyId, setBusyId] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  const fireNow = async (id: string) => {
    setBusyId(id);
    setError(null);
    try {
      await invoke('fire_deferred_now', { id });
    } catch (e) {
      setError(String(e));
    } finally {
      setBusyId(null);
    }
  };

  const cancel = async (id: string) => {
    setBusyId(id);
    setError(null);
    try {
      await invoke('cancel_deferred', { id });
    } catch (e) {
      setError(String(e));
    } finally {
      setBusyId(null);
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
            onCancel={cancel}
            isBusy={busyId === msg.id}
          />
        ))
      )}

      {scheduler.last_fired && (
        <div style={{ borderTop: '1px solid var(--border)', paddingTop: 6, color: 'var(--text-muted)', fontSize: 11 }}>
          last fired: <span style={{ color: 'var(--text)' }}>{scheduler.last_fired.message}</span>
        </div>
      )}

      <TestDeferButton />
    </div>
  );
}

function PendingRow({ msg, onFire, onCancel, isBusy }: {
  msg: DeferredMessage;
  onFire: (id: string) => void;
  onCancel: (id: string) => void;
  isBusy: boolean;
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
        disabled={isBusy}
        className="primary"
        style={{ fontSize: 10, padding: '2px 8px', flexShrink: 0 }}
        title="Fire this message immediately (dev shortcut)"
      >
        {isBusy ? '…' : 'fire now'}
      </button>
      <button
        onClick={() => onCancel(msg.id)}
        disabled={isBusy}
        style={{ fontSize: 10, padding: '2px 8px', flexShrink: 0 }}
        title="Cancel this deferred message"
      >
        cancel
      </button>
    </div>
  );
}
