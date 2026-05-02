import { useState } from 'react';
import { useDebugEvents } from '../../hooks/useDebugEvents';

const TAGS = ['[MEMORY]', '[AUDIO]', '[SCHEDULER]', '[ADAPTER]', '[ORCHESTRATOR]', '[MONITOR]'];

const TAG_COLORS: Record<string, string> = {
  '[MEMORY]':       '#9b59b6',
  '[AUDIO]':        '#27ae60',
  '[SCHEDULER]':    '#e67e22',
  '[ADAPTER]':      '#4a9eff',
  '[ORCHESTRATOR]': '#e8e8e8',
  '[MONITOR]':      '#777',
};

export function EventLog() {
  const { events, clear } = useDebugEvents();
  const [filter, setFilter] = useState('');

  const filtered = filter
    ? events.filter(e => e.component.includes(filter))
    : events;

  return (
    <div style={{ display: 'flex', flexDirection: 'column', height: '100%', minHeight: 200 }}>
      <div style={{ display: 'flex', gap: 6, marginBottom: 6, alignItems: 'center' }}>
        <div style={{ display: 'flex', gap: 4, flex: 1, flexWrap: 'wrap' }}>
          <button
            onClick={() => setFilter('')}
            style={{
              fontSize: 10, padding: '1px 6px',
              borderColor: !filter ? 'var(--accent)' : 'var(--border)',
              color: !filter ? 'var(--accent)' : 'var(--text-muted)',
            }}
          >
            all
          </button>
          {TAGS.map(tag => (
            <button
              key={tag}
              onClick={() => setFilter(f => f === tag ? '' : tag)}
              style={{
                fontSize: 10, padding: '1px 6px',
                borderColor: filter === tag ? TAG_COLORS[tag] ?? 'var(--accent)' : 'var(--border)',
                color: filter === tag ? TAG_COLORS[tag] ?? 'var(--accent)' : 'var(--text-muted)',
              }}
            >
              {tag}
            </button>
          ))}
        </div>
        <span style={{ color: 'var(--text-muted)', fontSize: 11 }}>{filtered.length}</span>
        <button onClick={clear} style={{ fontSize: 10, padding: '1px 6px' }}>clear</button>
      </div>

      <div style={{
        flex: 1, overflowY: 'auto', fontFamily: 'monospace', fontSize: 11,
        display: 'flex', flexDirection: 'column', gap: 1,
      }}>
        {filtered.length === 0 && (
          <div style={{ color: 'var(--text-muted)', padding: '8px 0' }}>no events</div>
        )}
        {filtered.map((e, i) => (
          <div key={i} style={{ display: 'flex', gap: 8, padding: '2px 0', lineHeight: 1.4 }}>
            <span style={{ color: '#555', flexShrink: 0 }}>{fmtTime(e.timestamp)}</span>
            <span style={{
              color: TAG_COLORS[e.component] ?? 'var(--text-muted)',
              flexShrink: 0, width: 100,
            }}>
              {e.component}
            </span>
            <span style={{ color: 'var(--text)', wordBreak: 'break-word' }}>{e.message}</span>
          </div>
        ))}
      </div>
    </div>
  );
}

function fmtTime(iso: string) {
  try { return new Date(iso).toLocaleTimeString([], { hour12: false }); } catch { return ''; }
}
