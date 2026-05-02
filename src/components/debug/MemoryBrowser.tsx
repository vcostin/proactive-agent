import { invoke } from '@tauri-apps/api/core';
import { useState } from 'react';

interface Props {
  onClose: () => void;
}

export function MemoryBrowser({ onClose }: Props) {
  const [query, setQuery] = useState('');
  const [results, setResults] = useState<string[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const search = async () => {
    if (!query.trim()) return;
    setLoading(true);
    setError(null);
    try {
      const r = await invoke<string[]>('get_memories', { query });
      setResults(r);
    } catch (e) {
      setError(String(e));
    } finally {
      setLoading(false);
    }
  };

  return (
    <div style={{
      position: 'fixed', inset: 0, background: 'rgba(0,0,0,0.75)',
      display: 'flex', alignItems: 'center', justifyContent: 'center',
      zIndex: 100,
    }}>
      <div style={{
        background: 'var(--bg-panel)', border: '1px solid var(--border)',
        borderRadius: 8, width: 600, maxHeight: '70vh',
        display: 'flex', flexDirection: 'column',
      }}>
        <div style={{
          display: 'flex', alignItems: 'center', gap: 8,
          padding: '10px 14px', borderBottom: '1px solid var(--border)',
        }}>
          <span style={{ flex: 1, fontWeight: 500, fontSize: 13 }}>Memory Browser</span>
          <button onClick={onClose} style={{ padding: '2px 8px', fontSize: 11 }}>✕ close</button>
        </div>

        <div style={{ padding: '10px 14px', borderBottom: '1px solid var(--border)', display: 'flex', gap: 8 }}>
          <input
            value={query}
            onChange={e => setQuery(e.target.value)}
            onKeyDown={e => e.key === 'Enter' && search()}
            placeholder="Search episodic + semantic memory…"
            style={{ flex: 1 }}
          />
          <button className="primary" onClick={search} disabled={loading || !query.trim()}>
            {loading ? '…' : 'search'}
          </button>
        </div>

        {error && (
          <div style={{ padding: '8px 14px', color: 'var(--error)', fontSize: 11 }}>{error}</div>
        )}

        <div style={{ flex: 1, overflowY: 'auto', padding: '10px 14px', display: 'flex', flexDirection: 'column', gap: 8 }}>
          {results.length === 0 && !loading && (
            <div style={{ color: 'var(--text-muted)', fontSize: 12, textAlign: 'center', marginTop: 20 }}>
              {query ? 'no results' : 'enter a query to search memory'}
            </div>
          )}
          {results.map((r, i) => (
            <div key={i} style={{
              padding: '8px 10px', background: 'var(--bg)',
              border: '1px solid var(--border)', borderRadius: 'var(--radius)',
              fontSize: 12, lineHeight: 1.6,
            }}>
              {r}
            </div>
          ))}
        </div>
      </div>
    </div>
  );
}
