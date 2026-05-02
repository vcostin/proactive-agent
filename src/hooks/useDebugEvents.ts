import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { useEffect, useState } from 'react';
import { DebugEvent } from '../types';

const MAX = 500;

export function useDebugEvents() {
  const [events, setEvents] = useState<DebugEvent[]>([]);

  useEffect(() => {
    let cancelled = false;
    let unlisten: (() => void) | null = null;

    // Seed from ring buffer on mount
    invoke<DebugEvent[]>('get_debug_events', { limit: MAX })
      .then(initial => { if (!cancelled) setEvents(initial); })
      .catch(() => {});

    // Live updates via Tauri event channel
    listen<DebugEvent>('debug_event', e => {
      if (cancelled) return;
      setEvents(prev => {
        const next = [e.payload, ...prev];
        return next.length > MAX ? next.slice(0, MAX) : next;
      });
    }).then(fn => {
      if (cancelled) fn();
      else unlisten = fn;
    });

    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, []);

  const clear = () => setEvents([]);

  return { events, clear };
}
