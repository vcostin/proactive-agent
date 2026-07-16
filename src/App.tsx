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
  /** Explicit Setup repair — reopens the wizard without clearing memory/config. */
  const [repairOpen, setRepairOpen] = useState(false);

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
    setRepairOpen(false);
    refreshStatus();
  };

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

  // Core agent gate lives in Rust (`ready` = chat model + llama). Piper is not required.
  // Forced re-entry when Core requirements are missing remains the safety net.
  const needsSetup = !setupStatus.ready;
  if (needsSetup || repairOpen) {
    return (
      <SetupWizard
        status={setupStatus}
        repair={repairOpen && setupStatus.ready}
        onComplete={handleSetupComplete}
        onClose={repairOpen ? () => { setRepairOpen(false); refreshStatus(); } : undefined}
      />
    );
  }

  return (
    <div style={{ display: 'flex', flexDirection: 'column', height: '100vh' }}>

      <RequirementsBanner />

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

        <button
          onClick={() => setRepairOpen(true)}
          style={{
            marginLeft: 8, fontSize: 11, padding: '4px 12px',
            color: 'var(--text-muted)',
          }}
          title="Re-check system prerequisites and app-managed artifacts"
        >
          Setup repair
        </button>

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

      <main style={{ flex: 1, overflow: 'hidden', position: 'relative' }}>
        <TabPanel visible={tab === 'chat'}>
          <ChatWindow
            modelName={modelBasename(activeModel)}
            onModelClick={() => setTab('models')}
          />
        </TabPanel>
        <TabPanel visible={tab === 'debug'}>
          <DebugPanel />
        </TabPanel>
        <TabPanel visible={tab === 'models'}>
          <ModelPanel
            activeModel={activeModel}
            onModelLoaded={path => { setActiveModel(path); setTab('chat'); }}
            onModelCleared={() => { setActiveModel(''); refreshStatus(); }}
          />
        </TabPanel>
      </main>
    </div>
  );
}

function TabPanel({ visible, children }: { visible: boolean; children: React.ReactNode }) {
  return (
    <div style={{
      position: 'absolute', inset: 0,
      display: 'flex', flexDirection: 'column',
      visibility: visible ? 'visible' : 'hidden',
      pointerEvents: visible ? 'auto' : 'none',
    }}>
      {children}
    </div>
  );
}

function modelBasename(path: string) {
  return path.split(/[\\/]/).pop() ?? path;
}
