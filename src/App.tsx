import { invoke } from '@tauri-apps/api/core';
import { useEffect, useState } from 'react';
import { ChatWindow } from './components/chat/ChatWindow';
import { DebugPanel } from './components/debug/DebugPanel';
import { ModelPanel } from './components/models/ModelPanel';
import { RequirementsBanner } from './components/setup/RequirementsBanner';
import { SetupWizard } from './components/setup/SetupWizard';
import { SetupStatus } from './types';

type Tab = 'chat' | 'debug' | 'models';

export default function App() {
  const [setupStatus, setSetupStatus] = useState<SetupStatus | null>(null);
  const [tab, setTab] = useState<Tab>('chat');
  const [activeModel, setActiveModel] = useState('');

  // Check setup on mount and whenever the window gains focus (catches
  // the case where the user downloads a model while the wizard is open)
  const refreshStatus = () => {
    invoke<SetupStatus>('get_setup_status').then(s => {
      setSetupStatus(s);
      if (s.chat_model) setActiveModel(s.chat_model);
    });
  };

  useEffect(() => {
    refreshStatus();
    window.addEventListener('focus', refreshStatus);
    return () => window.removeEventListener('focus', refreshStatus);
  }, []);

  const handleSetupComplete = (modelPath: string) => {
    setActiveModel(modelPath);
    refreshStatus();
  };

  // Still loading
  if (setupStatus === null) {
    return (
      <div style={{
        height: '100vh', display: 'flex', alignItems: 'center',
        justifyContent: 'center', color: 'var(--text-muted)', fontSize: 12,
      }}>
        starting…
      </div>
    );
  }

  // First run or no model selected
  if (!setupStatus.ready) {
    return <SetupWizard status={setupStatus} onComplete={handleSetupComplete} />;
  }

  // Main app
  return (
    <div style={{ display: 'flex', flexDirection: 'column', height: '100vh' }}>

      {/* ── Requirements banner (shown when llama-server can't start) ── */}
      <RequirementsBanner />

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

        {/* Active model pill */}
        {activeModel && (
          <button
            onClick={() => setTab('models')}
            style={{
              marginLeft: 'auto', fontSize: 10, padding: '2px 10px',
              color: 'var(--text-muted)', maxWidth: 260,
              overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap',
            }}
            title={activeModel}
          >
            {modelBasename(activeModel)}
          </button>
        )}
      </nav>

      {/* ── Content ── */}
      <main style={{ flex: 1, overflow: 'hidden', display: 'flex', flexDirection: 'column' }}>
        {tab === 'chat' && (
          <ChatWindow
            modelName={modelBasename(activeModel)}
            onModelClick={() => setTab('models')}
          />
        )}
        {tab === 'debug' && <DebugPanel />}
        {tab === 'models' && (
          <ModelPanel
            activeModel={activeModel}
            onModelLoaded={path => { setActiveModel(path); setTab('chat'); }}
            onModelCleared={() => { setActiveModel(''); refreshStatus(); }}
          />
        )}
      </main>
    </div>
  );
}

function modelBasename(path: string) {
  return path.split(/[\\/]/).pop() ?? path;
}
