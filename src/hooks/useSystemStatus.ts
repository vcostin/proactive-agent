import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { useEffect, useState } from 'react';
import { SystemStatus } from '../types';

export function useSystemStatus(intervalMs = 5000) {
  const [status, setStatus] = useState<SystemStatus | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let active = true;
    let unlisten: (() => void) | null = null;

    const poll = () => {
      invoke<SystemStatus>('get_system_status')
        .then(s => { if (active) setStatus(s); })
        .catch(e => { if (active) setError(String(e)); });
    };

    poll();
    const id = setInterval(poll, intervalMs);

    listen('scheduler_updated', () => {
      if (active) poll();
    }).then(fn => {
      if (!active) fn();
      else unlisten = fn;
    });

    return () => {
      active = false;
      clearInterval(id);
      unlisten?.();
    };
  }, [intervalMs]);

  return { status, error };
}
