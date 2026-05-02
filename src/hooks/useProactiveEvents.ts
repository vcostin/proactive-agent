import { listen } from '@tauri-apps/api/event';
import { useEffect } from 'react';
import { DeferredMessage } from '../types';

/**
 * Subscribe to proactive_message Tauri events.
 * `onMessage` is called each time the scheduler fires a deferred message.
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

    return () => {
      cancelled = true;
      unlisten?.();
    };
  // onMessage intentionally omitted — caller should stabilise with useCallback
  // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);
}
