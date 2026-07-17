import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { useEffect, useRef } from 'react';
import { DeferredMessage } from '../types';

/**
 * One shared drain for the app lifetime. React Strict Mode remounts the effect;
 * a second invoke would find an empty queue after the first drain.
 */
let flushDuePromise: Promise<DeferredMessage[]> | null = null;

function flushDueOnce(): Promise<DeferredMessage[]> {
  if (!flushDuePromise) {
    flushDuePromise = invoke<DeferredMessage[]>('flush_due_deferred');
  }
  return flushDuePromise;
}

/**
 * Subscribe to proactive_message Tauri events and flush any overdue deferred
 * messages that survived a restart (invoke return path — no event race).
 */
export function useProactiveEvents(onMessage: (msg: DeferredMessage) => void) {
  const onMessageRef = useRef(onMessage);
  onMessageRef.current = onMessage;

  useEffect(() => {
    let cancelled = false;
    let unlisten: (() => void) | null = null;

    listen<DeferredMessage>('proactive_message', event => {
      if (!cancelled) onMessageRef.current(event.payload);
    }).then(fn => {
      if (cancelled) fn();
      else unlisten = fn;
    });

    // Shared promise: Strict Mode's first mount may start the drain; only a
    // still-mounted effect delivers into that mount's chat (cancelled skips).
    flushDueOnce()
      .then(due => {
        if (cancelled) return;
        for (const msg of due) onMessageRef.current(msg);
      })
      .catch(e => console.error('[SCHEDULER] flush_due_deferred failed:', e));

    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, []);
}
