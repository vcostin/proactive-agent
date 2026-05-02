import { ModelInfo } from '../../types';

interface Props {
  models: ModelInfo[];
  activeModel: string;
  loading: string | null; // filename currently being loaded
  onLoad: (filename: string) => void;
}

export function ModelList({ models, activeModel, loading, onLoad }: Props) {
  if (models.length === 0) {
    return (
      <div style={{ padding: 24, color: 'var(--text-muted)', textAlign: 'center' }}>
        No .gguf files found in the models/ directory.
      </div>
    );
  }

  return (
    <table style={{ width: '100%', borderCollapse: 'collapse', fontSize: 12 }}>
      <thead>
        <tr style={{ borderBottom: '1px solid var(--border)', color: 'var(--text-muted)' }}>
          {['File', 'Quant', 'Params', 'Size', 'Modified', ''].map(h => (
            <th key={h} style={{ padding: '6px 10px', textAlign: 'left', fontWeight: 'normal' }}>
              {h}
            </th>
          ))}
        </tr>
      </thead>
      <tbody>
        {models.map(m => {
          const isActive = m.filename === activeModel;
          const isLoading = m.filename === loading;
          return (
            <tr
              key={m.filename}
              style={{
                borderBottom: '1px solid var(--border)',
                background: isActive ? 'var(--accent-dim)' : 'transparent',
              }}
            >
              <td style={{ padding: '7px 10px', color: isActive ? 'var(--accent)' : 'var(--text)' }}>
                {isActive && <span style={{ marginRight: 6 }}>▶</span>}
                {m.filename}
              </td>
              <td style={{ padding: '7px 10px', color: 'var(--text-muted)' }}>{m.quant_type}</td>
              <td style={{ padding: '7px 10px', color: 'var(--text-muted)' }}>{m.param_count}</td>
              <td style={{ padding: '7px 10px', color: 'var(--text-muted)' }}>{fmtBytes(m.file_size_bytes)}</td>
              <td style={{ padding: '7px 10px', color: 'var(--text-muted)' }}>{fmtDate(m.last_modified)}</td>
              <td style={{ padding: '7px 10px' }}>
                <button
                  className={isActive ? '' : 'primary'}
                  disabled={isActive || isLoading !== false}
                  onClick={() => onLoad(m.filename)}
                  style={{ fontSize: 11, padding: '2px 10px' }}
                >
                  {isLoading ? '…' : isActive ? 'active' : 'load'}
                </button>
              </td>
            </tr>
          );
        })}
        {/* EXTEND: fine-tune management row (future) */}
        <tr style={{ borderBottom: '1px solid var(--border)', opacity: 0.3 }}>
          <td colSpan={6} style={{ padding: '7px 10px', fontStyle: 'italic', fontSize: 11 }}>
            Fine-tuning — future
          </td>
        </tr>
      </tbody>
    </table>
  );
}

function fmtBytes(n: number) {
  if (n >= 1024 ** 3) return `${(n / 1024 ** 3).toFixed(1)} GB`;
  if (n >= 1024 ** 2) return `${(n / 1024 ** 2).toFixed(0)} MB`;
  return `${(n / 1024).toFixed(0)} KB`;
}

function fmtDate(iso: string) {
  try { return new Date(iso).toLocaleDateString(); } catch { return iso; }
}
