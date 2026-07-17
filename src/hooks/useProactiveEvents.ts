import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { useEffect } from 'react';
import { DeferredMessage } from '../types';

/**
 * Subscribe to proactive_message Tauri events and flush any overdue deferred
 * messages that survived a restart (invoke return path — no event race).
 */
export function useProactiveEvents(onMessage: (msg: DeferredMessage) => void) {
  useEffect(() => {
    let cancelled = false;
    let unlisten: (() => void) | null = null;

    listen<DeferredMessage>('proactive_message', event => {
      if (!cancelled) onMessage(event.payload);
    }).then(fn => {
      if (cancelled) fn();
      else unlisten = fn;
    });

    // Deliver even if this effect cleaned up (React Strict Mode remount):
    // drain is one-shot under a mutex — dropping results would lose overdue nudges.
    invoke<DeferredMessage[]>('flush_due_deferred')
      .then(due => {
        for (const msg of due) onMessage(msg);
      })
      .catch(e => console.error('[SCHEDULER] flush_due_deferred failed:', e));

    return () => {
      cancelled = true;
      unlisten?.();
    };
  // onMessage intentionally omitted — caller should stabilise with useCallback
  // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);
}
