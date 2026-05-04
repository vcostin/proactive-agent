import { open } from '@tauri-apps/plugin-dialog';
import { invoke } from '@tauri-apps/api/core';
import { useEffect, useState } from 'react';
import { ModelInfo } from '../../types';
import { ModelList } from './ModelList';

interface GenSettings { temperature: number; top_p: number; context_window_tokens: number; }

interface Props {
  activeModel: string;
  onModelLoaded: (path: string) => void;
  onModelCleared: () => void;
}

export function ModelPanel({ activeModel, onModelLoaded, onModelCleared }: Props) {
  const [models, setModels] = useState<ModelInfo[]>([]);
  const [loading, setLoading] = useState<string | null>(null);
  const [clearing, setClearing] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [settings, setSettings] = useState<GenSettings>({ temperature: 0.7, top_p: 0.95, context_window_tokens: 4096 });
  const [settingsSaved, setSettingsSaved] = useState(false);

  useEffect(() => {
    invoke<GenSettings>('get_gen_settings').then(setSettings).catch(() => {});
  }, []);

  const saveSettings = async () => {
    try {
      await invoke('set_gen_settings', { settings });
      setSettingsSaved(true);
      setTimeout(() => setSettingsSaved(false), 1500);
    } catch (e) { setError(String(e)); }
  };

  const refresh = () => {
    invoke<ModelInfo[]>('list_models')
      .then(setModels)
      .catch(e => setError(String(e)));
  };

  useEffect(() => { refresh(); }, []);

  const loadModel = async (path: string) => {
    setLoading(path);
    setError(null);
    try {
      await invoke('swap_model', { modelPath: path });
      onModelLoaded(path);
      refresh();
    } catch (e) {
      setError(String(e));
    } finally {
      setLoading(null);
    }
  };

  const handleBrowse = async () => {
    setError(null);
    try {
      const path = await open({
        title: 'Select a chat model',
        filters: [{ name: 'GGUF Model', extensions: ['gguf'] }],
        multiple: false,
      });
      if (path && typeof path === 'string') {
        await loadModel(path);
      }
    } catch (e) {
      setError(String(e));
    }
  };

  return (
    <div style={{ height: '100%', display: 'flex', flexDirection: 'column' }}>

      {/* Header */}
      <div style={{
        display: 'flex', alignItems: 'center', gap: 10,
        padding: '10px 16px', borderBottom: '1px solid var(--border)',
        background: 'var(--bg-panel)',
      }}>
        <span style={{ flex: 1, fontWeight: 500 }}>Models</span>
        <button onClick={refresh} style={{ fontSize: 11, padding: '2px 8px' }}>
          ↻ refresh
        </button>
        {activeModel && (
          <button
            onClick={async () => {
              if (!confirm('Unload the current model and return to setup?')) return;
              setClearing(true);
              try {
                await invoke('clear_model');
                onModelCleared();
              } catch (e) { setError(String(e)); }
              finally { setClearing(false); }
            }}
            disabled={clearing}
            style={{ fontSize: 11, padding: '4px 14px', color: 'var(--error)', borderColor: 'var(--error)' }}
          >
            {clearing ? '…' : 'unload'}
          </button>
        )}
        <button className="primary" onClick={handleBrowse} style={{ fontSize: 11, padding: '4px 14px' }}>
          + Browse for .gguf…
        </button>
      </div>

      {error && (
        <div style={{
          padding: '8px 16px', color: 'var(--error)',
          borderBottom: '1px solid var(--border)', fontSize: 11,
        }}>
          {error}
        </div>
      )}

      {models.length === 0 && (
        <div style={{
          flex: 1, display: 'flex', flexDirection: 'column',
          alignItems: 'center', justifyContent: 'center', gap: 12,
          color: 'var(--text-muted)',
        }}>
          <span style={{ fontSize: 13 }}>No models loaded yet</span>
          <button className="primary" onClick={handleBrowse} style={{ padding: '8px 20px' }}>
            Browse for .gguf file…
          </button>
          <span style={{ fontSize: 11, maxWidth: 320, textAlign: 'center', lineHeight: 1.6 }}>
            Download a model from{' '}
            <span style={{ color: 'var(--accent)' }}>huggingface.co</span>
            {' '}or{' '}
            <span style={{ color: 'var(--accent)' }}>lmstudio.ai</span>
            {' '}and pick the .gguf file from anywhere on your disk.
          </span>
        </div>
      )}

      {models.length > 0 && (
        <div style={{ flex: 1, overflowY: 'auto' }}>
          <ModelList
            models={models}
            activeModel={activeModel}
            loading={loading}
            onLoad={loadModel}
          />
        </div>
      )}
      {/* Generation parameters */}
      <div style={{ padding: '10px 16px', borderTop: '1px solid var(--border)' }}>
        <div style={{ fontSize: 10, color: 'var(--text-muted)', textTransform: 'uppercase', letterSpacing: '0.08em', marginBottom: 8 }}>
          Generation parameters
        </div>
        <div style={{ display: 'flex', flexDirection: 'column', gap: 8, fontSize: 12 }}>
          {[
            { key: 'temperature' as const, label: 'Temperature', min: 0, max: 2, step: 0.05 },
            { key: 'top_p' as const,       label: 'Top-P',       min: 0, max: 1, step: 0.05 },
          ].map(({ key, label, min, max, step }) => (
            <div key={key} style={{ display: 'flex', alignItems: 'center', gap: 10 }}>
              <span style={{ color: 'var(--text-muted)', width: 90 }}>{label}</span>
              <input
                type="range" min={min} max={max} step={step}
                value={settings[key]}
                onChange={e => setSettings(s => ({ ...s, [key]: parseFloat(e.target.value) }))}
                style={{ flex: 1 }}
              />
              <span style={{ width: 36, textAlign: 'right', color: 'var(--text)' }}>
                {settings[key].toFixed(2)}
              </span>
            </div>
          ))}
          <div style={{ display: 'flex', alignItems: 'center', gap: 10 }}>
            <span style={{ color: 'var(--text-muted)', width: 90 }}>Context</span>
            <input
              type="range" min={512} max={32768} step={512}
              value={settings.context_window_tokens}
              onChange={e => setSettings(s => ({ ...s, context_window_tokens: parseInt(e.target.value) }))}
              style={{ flex: 1 }}
            />
            <span style={{ width: 36, textAlign: 'right', color: 'var(--text)' }}>
              {settings.context_window_tokens >= 1024
                ? `${(settings.context_window_tokens / 1024).toFixed(0)}k`
                : settings.context_window_tokens}
            </span>
          </div>
          <button
            className={settingsSaved ? '' : 'primary'}
            onClick={saveSettings}
            style={{ alignSelf: 'flex-end', fontSize: 11, padding: '3px 14px' }}
          >
            {settingsSaved ? '✓ saved' : 'apply'}
          </button>
        </div>
      </div>

    </div>
  );
}
