import { invoke } from '@tauri-apps/api/core';
import { useState } from 'react';
import { useSystemStatus } from '../../hooks/useSystemStatus';
import { SystemRequirements } from '../setup/SystemRequirements';
import { AudioState } from './AudioState';
import { ContextInspector } from './ContextInspector';
import { EventLog } from './EventLog';
import { MemoryBrowser } from './MemoryBrowser';
import { MemoryStats } from './MemoryStats';
import { SchedulerPanel } from './SchedulerPanel';
import { SidecarHealth } from './SidecarHealth';

export function DebugPanel() {
  const { status, error } = useSystemStatus();
  const [showBrowser, setShowBrowser] = useState(false);
  const [showDevTools, setShowDevTools] = useState(false);

  return (
    <div style={{ height: '100%', display: 'flex', flexDirection: 'column', overflow: 'hidden' }}>

      {/* Header */}
      <div style={{
        display: 'flex', alignItems: 'center', gap: 10,
        padding: '7px 14px', borderBottom: '1px solid var(--border)',
        background: 'var(--bg-panel)', flexShrink: 0,
      }}>
        <span style={{ fontWeight: 500 }}>Debug</span>
        <span style={{ fontSize: 10, color: 'var(--text-muted)' }}>status polled every 5s</span>
        {error && <span style={{ fontSize: 11, color: 'var(--error)' }}>⚠ {error}</span>}
        <div style={{ flex: 1 }} />
        {status?.active_model && (
          <span style={{ fontSize: 11, color: 'var(--text-muted)' }}>
            {status.active_model.filename} · {status.active_model.quant_type} · {status.active_model.param_count}
          </span>
        )}
      </div>

      {/* Scrollable body */}
      <div style={{
        flex: 1, overflow: 'auto', padding: 12,
        display: 'grid',
        gridTemplateColumns: '1fr 1fr',
        gridAutoRows: 'max-content',
        gap: 10,
        alignContent: 'start',
      }}>
        <Panel title="Sidecar Health">
          <SidecarHealth />
        </Panel>

        <Panel title="System Requirements">
          <SystemRequirements />
        </Panel>

        <Panel title="Scheduler">
          <SchedulerPanel scheduler={status?.scheduler ?? { pending: [], last_fired: null }} />
        </Panel>

        <Panel title="Memory">
          <MemoryStats
            stats={status?.memory ?? {
              episodic_count: 0, semantic_count: 0,
              last_write: null, last_distillation: null, last_embed_latency_ms: 0,
            }}
            onBrowse={() => setShowBrowser(true)}
          />
        </Panel>

        <Panel title="Context Inspector">
          <ContextInspector />
        </Panel>

        <Panel title="Audio">
          <AudioState audio={status?.audio ?? {
            device_name: '—', sample_rate: 0, channels: 0,
            vad_state: 'Silent', energy_level: 0,
            last_stt_result: null, last_stt_latency_ms: 0,
            last_tts_latency_ms: 0, tts_buffer_fill_pct: 0,
          }} />
        </Panel>

        {/* Event log spans full width */}
        <div style={{ gridColumn: '1 / -1' }}>
          <Panel title="Event Log">
            <EventLog />
          </Panel>
        </div>

        {/* Dev tools — collapsed by default, not visible in production UX */}
        <div style={{ gridColumn: '1 / -1' }}>
          <button
            onClick={() => setShowDevTools(v => !v)}
            style={{ fontSize: 10, padding: '2px 8px', color: 'var(--text-muted)', width: '100%', textAlign: 'left' }}
          >
            {showDevTools ? '▾' : '▸'} dev tools
          </button>
          {showDevTools && (
            <div style={{ marginTop: 6, display: 'grid', gridTemplateColumns: '1fr 1fr', gap: 10 }}>
              <Panel title="Port Diagnostic">
                <DiagnosticButton />
              </Panel>
            </div>
          )}
        </div>
      </div>

      {showBrowser && <MemoryBrowser onClose={() => setShowBrowser(false)} />}
    </div>
  );
}

function DiagnosticButton() {
  const [result, setResult] = useState('');
  const [running, setRunning] = useState(false);
  return (
    <div style={{ marginTop: 8 }}>
      <button
        style={{ fontSize: 11, padding: '2px 10px' }}
        disabled={running}
        onClick={async () => {
          setRunning(true);
          try { setResult(await invoke<string>('diagnose_chat_server')); }
          catch (e) { setResult(String(e)); }
          finally { setRunning(false); }
        }}
      >
        {running ? '…' : '🔍 diagnose port 8080'}
      </button>
      {result && (
        <pre style={{
          marginTop: 6, padding: 8, background: '#0a0a0a',
          border: '1px solid var(--border)', borderRadius: 4,
          fontSize: 10, lineHeight: 1.5, overflowX: 'auto', whiteSpace: 'pre-wrap',
        }}>
          {result}
        </pre>
      )}
    </div>
  );
}

function Panel({ title, children }: { title: string; children: React.ReactNode }) {
  return (
    <div style={{
      background: 'var(--bg-panel)',
      border: '1px solid var(--border)',
      borderRadius: 'var(--radius)',
      overflow: 'hidden',
    }}>
      <div style={{
        padding: '5px 10px',
        borderBottom: '1px solid var(--border)',
        fontSize: 10,
        color: 'var(--text-muted)',
        textTransform: 'uppercase',
        letterSpacing: '0.08em',
      }}>
        {title}
      </div>
      <div style={{ padding: 10 }}>{children}</div>
    </div>
  );
}
