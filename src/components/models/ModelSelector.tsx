import { invoke } from '@tauri-apps/api/core';
import { useEffect, useState } from 'react';
import { ModelInfo } from '../../types';

interface Props {
  activeModel: string;
  onSwapped: (filename: string) => void;
}

/** Compact hot-swap widget used in the chat header. */
export function ModelSelector({ activeModel, onSwapped }: Props) {
  const [models, setModels] = useState<ModelInfo[]>([]);
  const [swapping, setSwapping] = useState(false);

  useEffect(() => {
    invoke<ModelInfo[]>('list_models').then(setModels).catch(() => {});
  }, []);

  const handleChange = async (filename: string) => {
    if (!filename || filename === activeModel) return;
    setSwapping(true);
    try {
      await invoke('swap_model', { modelFilename: filename });
      onSwapped(filename);
    } catch (e) {
      console.error('swap_model failed:', e);
    } finally {
      setSwapping(false);
    }
  };

  if (models.length === 0) {
    return (
      <span style={{ fontSize: 11, color: 'var(--text-muted)' }}>
        {activeModel || 'no models found'}
      </span>
    );
  }

  return (
    <select
      value={activeModel}
      onChange={e => handleChange(e.target.value)}
      disabled={swapping}
      style={{ fontSize: 11, padding: '2px 6px' }}
      title="Hot-swap chat model"
    >
      {!activeModel && <option value="">— select model —</option>}
      {models.map(m => (
        <option key={m.filename} value={m.filename}>
          {m.filename} ({m.quant_type} · {m.param_count})
        </option>
      ))}
    </select>
  );
}
