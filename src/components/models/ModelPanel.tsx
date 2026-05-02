import { invoke } from '@tauri-apps/api/core';
import { useEffect, useState } from 'react';
import { ModelInfo } from '../../types';
import { ModelList } from './ModelList';

interface Props {
  activeModel: string;
  onModelLoaded: (filename: string) => void;
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

  const handleLoad = async (filename: string) => {
    setLoading(filename);
    setError(null);
    try {
      await invoke('swap_model', { modelFilename: filename });
      onModelLoaded(filename);
    } catch (e) {
      setError(String(e));
    } finally {
      setLoading(null);
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
        <span style={{ fontSize: 11, color: 'var(--text-muted)' }}>
          {models.length} file{models.length !== 1 ? 's' : ''} · models/
        </span>
        <button onClick={refresh} style={{ fontSize: 11, padding: '2px 8px' }}>
          ↻ refresh
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

      <div style={{ flex: 1, overflowY: 'auto' }}>
        <ModelList
          models={models}
          activeModel={activeModel}
          loading={loading}
          onLoad={handleLoad}
        />
      </div>
    </div>
  );
}
