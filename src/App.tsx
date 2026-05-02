import { useState } from 'react';
import { ChatWindow } from './components/chat/ChatWindow';
import { ModelPanel } from './components/models/ModelPanel';

type Tab = 'chat' | 'debug' | 'models';

export default function App() {
  const [tab, setTab] = useState<Tab>('chat');
  const [activeModel, setActiveModel] = useState('');

  return (
    <div style={{ display: 'flex', flexDirection: 'column', height: '100vh' }}>

      {/* ── Tab bar ── */}
      <nav style={{
        display: 'flex', alignItems: 'center', gap: 4,
        padding: '6px 12px', borderBottom: '1px solid var(--border)',
        background: 'var(--bg-panel)', flexShrink: 0,
      }}>
        {(['chat', 'debug', 'models'] as Tab[]).map(t => (
          <button
            key={t}
            onClick={() => setTab(t)}
            style={{
              padding: '4px 14px',
              background: tab === t ? 'var(--accent-dim)' : 'transparent',
              borderColor: tab === t ? 'var(--accent)' : 'var(--border)',
              color: tab === t ? 'var(--accent)' : 'var(--text-muted)',
            }}
          >
            {t}
          </button>
        ))}
      </nav>

      {/* ── Content ── */}
      <main style={{ flex: 1, overflow: 'hidden', display: 'flex', flexDirection: 'column' }}>
        {tab === 'chat' && (
          <ChatWindow
            modelName={activeModel}
            onModelClick={() => setTab('models')}
          />
        )}
        {tab === 'debug' && <DebugPlaceholder />}
        {tab === 'models' && (
          <ModelPanel
            activeModel={activeModel}
            onModelLoaded={name => { setActiveModel(name); setTab('chat'); }}
          />
        )}
      </main>
    </div>
  );
}

function DebugPlaceholder() {
  return (
    <div style={{
      flex: 1, display: 'flex', alignItems: 'center',
      justifyContent: 'center', flexDirection: 'column', gap: 8,
      color: 'var(--text-muted)',
    }}>
      <span style={{ fontSize: 16 }}>debug</span>
      <span style={{ fontSize: 11 }}>Phase 6</span>
    </div>
  );
}
