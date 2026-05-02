import { open } from '@tauri-apps/plugin-dialog';
import { invoke } from '@tauri-apps/api/core';
import { useEffect, useState } from 'react';
import { ModelInfo } from '../../types';
import { ModelList } from './ModelList';

interface Props {
  activeModel: string;
  onModelLoaded: (path: string) => void;
}

export function ModelPanel({ activeModel, onModelLoaded }: Props) {
  const [models, setModels] = useState<ModelInfo[]>([]);
  const [loading, setLoading] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

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
    </div>
  );
}
