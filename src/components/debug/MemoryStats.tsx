import { MemoryStats as IMemoryStats } from '../../types';

interface Props {
  stats: IMemoryStats;
  onBrowse: () => void;
}

export function MemoryStats({ stats, onBrowse }: Props) {
  return (
    <div style={{ display: 'flex', flexDirection: 'column', gap: 6, fontSize: 12 }}>
      <Row label="episodic entries" value={stats.episodic_count.toLocaleString()} />
      <Row label="semantic facts" value={stats.semantic_count.toLocaleString()} />
      <Row label="last write" value={fmtTs(stats.last_write)} />
      <Row label="last distillation" value={fmtTs(stats.last_distillation)} />
      <Row
        label="embed latency"
        value={`${stats.last_embed_latency_ms} ms`}
        highlight={stats.last_embed_latency_ms > 500}
      />
      <div style={{ marginTop: 4 }}>
        <button onClick={onBrowse} style={{ fontSize: 11, padding: '2px 10px' }}>
          browse memory
        </button>
      </div>
    </div>
  );
}

function Row({ label, value, highlight = false }: { label: string; value: string; highlight?: boolean }) {
  return (
    <div style={{ display: 'flex', justifyContent: 'space-between', gap: 8 }}>
      <span style={{ color: 'var(--text-muted)' }}>{label}</span>
      <span style={{ color: highlight ? '#c47a00' : 'var(--text)' }}>{value}</span>
    </div>
  );
}

function fmtTs(iso: string | null) {
  if (!iso) return '—';
  try { return new Date(iso).toLocaleTimeString(); } catch { return iso; }
}
