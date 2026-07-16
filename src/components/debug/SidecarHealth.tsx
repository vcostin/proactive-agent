import { listen } from '@tauri-apps/api/event';
import { useEffect, useState } from 'react';
import { SidecarHealthEvent } from '../../types';

const SIDECARS = ['llama', 'embed'];

export function SidecarHealth() {
  const [health, setHealth] = useState<Record<string, SidecarHealthEvent>>({});

  useEffect(() => {
    let unlisten: (() => void) | null = null;
    listen<SidecarHealthEvent>('sidecar_health', e => {
      setHealth(prev => ({ ...prev, [e.payload.name]: e.payload }));
    }).then(fn => { unlisten = fn; });
    return () => { unlisten?.(); };
  }, []);

  return (
    <div style={{ display: 'flex', flexDirection: 'column', gap: 6 }}>
      {SIDECARS.map(name => {
        const s = health[name];
        const state = s?.state ?? null;
        const dot   = state === null     ? '#555'
                    : state === 'online'  ? '#55c477'
                    : state === 'loading' ? '#c4a000'
                    :                       '#e05555';
        const glow  = state === 'online'  || state === 'loading';
        const label = state === 'loading' ? 'loading…' : null;
        const latency = s?.latency_ms ?? '—';
        const port    = s?.port ?? '—';

        return (
          <div key={name} style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
            <span style={{
              width: 8, height: 8, borderRadius: '50%',
              background: dot, flexShrink: 0,
              boxShadow: glow ? `0 0 4px ${dot}` : 'none',
              animation: state === 'loading' ? 'blink 1.2s ease-in-out infinite' : 'none',
            }} />
            <span style={{ width: 60, color: 'var(--text)' }}>{name}</span>
            <span style={{ color: 'var(--text-muted)', fontSize: 11 }}>:{port}</span>
            {label && (
              <span style={{ fontSize: 10, color: '#c4a000' }}>{label}</span>
            )}
            <span style={{ flex: 1 }} />
            {s && !label && (
              <span style={{ fontSize: 11, color: (s.latency_ms > 500 ? '#c47a00' : 'var(--text-muted)') }}>
                {latency} ms
              </span>
            )}
            {s && (
              <span style={{ fontSize: 10, color: 'var(--text-muted)', width: 32, textAlign: 'right' }}>
                {s.status_code || '—'}
              </span>
            )}
          </div>
        );
      })}
    </div>
  );
}
