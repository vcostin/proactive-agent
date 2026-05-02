import { invoke } from '@tauri-apps/api/core';
import { useEffect, useState } from 'react';
import { SystemStatus } from '../types';

export function useSystemStatus(intervalMs = 5000) {
  const [status, setStatus] = useState<SystemStatus | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let active = true;

    const poll = () => {
      invoke<SystemStatus>('get_system_status')
        .then(s => { if (active) setStatus(s); })
        .catch(e => { if (active) setError(String(e)); });
    };

    poll();
    const id = setInterval(poll, intervalMs);
    return () => { active = false; clearInterval(id); };
  }, [intervalMs]);

  return { status, error };
}
