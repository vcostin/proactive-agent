import { invoke } from '@tauri-apps/api/core';
import { useEffect, useState } from 'react';
import { AssembledContext, ContextBlock } from '../../types';

const CONTEXT_WINDOW = 4096; // mirror AppConfig default

const SECTION_COLORS: Record<string, string> = {
  persona:  '#4a9eff',
  semantic: '#9b59b6',
  episodic: '#e67e22',
  recent:   '#27ae60',
  input:    '#e8e8e8',
};

export function ContextInspector() {
  const [ctx, setCtx] = useState<AssembledContext | null>(null);
  const [expanded, setExpanded] = useState<Record<string, boolean>>({});
  const [loading, setLoading] = useState(false);

  const refresh = () => {
    setLoading(true);
    invoke<AssembledContext | null>('get_last_context')
      .then(setCtx)
      .finally(() => setLoading(false));
  };

  useEffect(() => { refresh(); }, []);

  if (!ctx) {
    return (
      <div style={{ color: 'var(--text-muted)', fontSize: 12, textAlign: 'center', padding: 12 }}>
        {loading ? 'loading…' : 'no context yet — send a message first'}
        {!loading && (
          <div style={{ marginTop: 8 }}>
            <button onClick={refresh} style={{ fontSize: 11 }}>refresh</button>
          </div>
        )}
      </div>
    );
  }

  const totalTokens = ctx.persona.token_count + ctx.semantic.token_count +
    ctx.episodic.token_count +
    ctx.recent_turns.reduce((s, t) => s + Math.ceil(t.content.split(/\s+/).length * 4 / 3), 0) +
    Math.ceil(ctx.user_input.split(/\s+/).length * 4 / 3);

  const sections: { key: string; block: ContextBlock }[] = [
    { key: 'persona',  block: ctx.persona },
    { key: 'semantic', block: ctx.semantic },
    { key: 'episodic', block: ctx.episodic },
  ];

  return (
    <div style={{ display: 'flex', flexDirection: 'column', gap: 6, fontSize: 12 }}>
      <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginBottom: 2 }}>
        <span style={{ color: 'var(--text-muted)', fontSize: 11 }}>
          {totalTokens} / {CONTEXT_WINDOW} tokens
        </span>
        <button onClick={refresh} style={{ fontSize: 10, padding: '1px 6px' }}>↻</button>
      </div>

      {/* Total token bar */}
      <div style={{ height: 4, background: '#222', borderRadius: 2, overflow: 'hidden', marginBottom: 4 }}>
        <div style={{
          height: '100%',
          width: `${Math.min(100, (totalTokens / CONTEXT_WINDOW) * 100)}%`,
          background: totalTokens > CONTEXT_WINDOW * 0.9 ? 'var(--error)' : 'var(--accent)',
        }} />
      </div>

      {sections.map(({ key, block }) => (
        <Section
          key={key}
          label={block.label}
          tokens={block.token_count}
          total={CONTEXT_WINDOW}
          color={SECTION_COLORS[key] ?? '#888'}
          content={block.content}
          expanded={!!expanded[key]}
          onToggle={() => setExpanded(e => ({ ...e, [key]: !e[key] }))}
        />
      ))}

      {/* Recent turns */}
      <Section
        label={`recent turns (${ctx.recent_turns.length})`}
        tokens={ctx.recent_turns.reduce((s, t) => s + Math.ceil(t.content.split(/\s+/).length * 4 / 3), 0)}
        total={CONTEXT_WINDOW}
        color={SECTION_COLORS['recent']}
        content={ctx.recent_turns.map(t => `[${t.role}] ${t.content}`).join('\n\n')}
        expanded={!!expanded['recent']}
        onToggle={() => setExpanded(e => ({ ...e, recent: !e['recent'] }))}
      />

      {/* Current input */}
      <Section
        label="user input"
        tokens={Math.ceil(ctx.user_input.split(/\s+/).length * 4 / 3)}
        total={CONTEXT_WINDOW}
        color={SECTION_COLORS['input']}
        content={ctx.user_input}
        expanded={!!expanded['input']}
        onToggle={() => setExpanded(e => ({ ...e, input: !e['input'] }))}
      />
    </div>
  );
}

function Section({ label, tokens, total, color, content, expanded, onToggle }: {
  label: string; tokens: number; total: number; color: string;
  content: string; expanded: boolean; onToggle: () => void;
}) {
  const pct = Math.min(100, (tokens / total) * 100);
  return (
    <div style={{ border: '1px solid var(--border)', borderRadius: 'var(--radius)', overflow: 'hidden' }}>
      <button
        onClick={onToggle}
        style={{
          width: '100%', display: 'flex', alignItems: 'center', gap: 8,
          padding: '5px 8px', background: 'transparent', border: 'none',
          borderRadius: 0, textAlign: 'left', cursor: 'pointer',
        }}
      >
        <span style={{ color, width: 8, flexShrink: 0 }}>{expanded ? '▾' : '▸'}</span>
        <span style={{ flex: 1, color: 'var(--text)' }}>{label}</span>
        <span style={{ color: 'var(--text-muted)', fontSize: 11 }}>{tokens} tok</span>
        <div style={{ width: 48, height: 3, background: '#222', borderRadius: 2, overflow: 'hidden' }}>
          <div style={{ height: '100%', width: `${pct}%`, background: color }} />
        </div>
      </button>
      {expanded && content && (
        <pre style={{
          margin: 0, padding: '6px 10px',
          background: '#0a0a0a', color: 'var(--text-muted)',
          fontSize: 11, lineHeight: 1.5, overflowX: 'auto',
          whiteSpace: 'pre-wrap', wordBreak: 'break-word',
          borderTop: '1px solid var(--border)',
          maxHeight: 200, overflowY: 'auto',
        }}>
          {content || '(empty)'}
        </pre>
      )}
    </div>
  );
}
