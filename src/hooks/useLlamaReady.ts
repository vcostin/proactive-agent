import { listen } from '@tauri-apps/api/event';
import { useEffect, useState } from 'react';
import { SidecarHealthEvent } from '../types';

/**
 * Tracks whether the chat llama-server (port 18080) is responding.
 * The monitor loop emits sidecar_health every ~2s.
 * Returns false while the model is loading, true once it's serving requests.
 */
export function useLlamaReady() {
  const [ready, setReady] = useState(false);

  useEffect(() => {
    let unlisten: (() => void) | null = null;
    listen<SidecarHealthEvent>('sidecar_health', e => {
      if (e.payload.name === 'llama') {
        setReady(e.payload.alive);
      }
    }).then(fn => { unlisten = fn; });
    return () => { unlisten?.(); };
  }, []);

  return ready;
}
