import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { useCallback, useEffect, useRef, useState } from 'react';
import { ChatMessage } from '../types';

const HISTORY_KEY = 'proactive_chat_history';
const MAX_HISTORY = 200;

let msgCounter = 0;
function nextId() { return `msg-${++msgCounter}`; }

function loadHistory(): ChatMessage[] {
  try {
    const raw = localStorage.getItem(HISTORY_KEY);
    return raw ? JSON.parse(raw) : [];
  } catch { return []; }
}

function saveHistory(msgs: ChatMessage[]) {
  try {
    localStorage.setItem(HISTORY_KEY, JSON.stringify(msgs.slice(-MAX_HISTORY)));
  } catch { /* storage full */ }
}

export function useChat() {
  const [messages, setMessages] = useState<ChatMessage[]>(loadHistory);
  const [streamingText, setStreamingText] = useState<string | null>(null);
  const [isLoading, setIsLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [ttsEnabled, setTtsEnabled] = useState(false);
  const ttsEnabledRef = useRef(false);
  useEffect(() => { ttsEnabledRef.current = ttsEnabled; }, [ttsEnabled]);
  // Keep a ref to messages so the event listener always sees current value
  const messagesRef = useRef(messages);
  useEffect(() => { messagesRef.current = messages; }, [messages]);

  // Subscribe to streaming token events
  useEffect(() => {
    let unlistenToken: (() => void) | null = null;
    let unlistenVoice: (() => void) | null = null;
    let cancelled = false;

    listen<string>('chat_token', e => {
      if (!cancelled) setStreamingText(prev => (prev ?? '') + e.payload);
    }).then(fn => { if (cancelled) fn(); else unlistenToken = fn; });

    // Voice transcripts route into sendMessage just like keyboard input
    listen<string>('voice_transcript', e => {
      if (!cancelled && e.payload.trim()) sendMessage(e.payload.trim());
    }).then(fn => { if (cancelled) fn(); else unlistenVoice = fn; });

    return () => {
      cancelled = true;
      unlistenToken?.();
      unlistenVoice?.();
    };
  // sendMessage is stable (wrapped in useCallback) so this effect only runs once
  // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const sendMessage = useCallback(async (text: string) => {
    const trimmed = text.trim();
    if (!trimmed || isLoading) return;

    const userMsg: ChatMessage = {
      id: nextId(), role: 'user', content: trimmed,
      timestamp: new Date().toISOString(),
    };
    const next = [...messagesRef.current, userMsg];
    setMessages(next);
    saveHistory(next);
    setIsLoading(true);
    setStreamingText('');    // start streaming bubble
    setError(null);

    try {
      const response = await invoke<string>('send_message', { message: trimmed });
      const assistantMsg: ChatMessage = {
        id: nextId(), role: 'assistant', content: response,
        timestamp: new Date().toISOString(),
      };
      const final = [...messagesRef.current, assistantMsg];
      setMessages(final);
      saveHistory(final);
      // Use ref so the closure always sees the current toggle state
      if (ttsEnabledRef.current && response.trim()) {
        invoke('speak_text', { text: response }).catch(() => {});
      }
    } catch (e) {
      setError(String(e));
    } finally {
      setIsLoading(false);
      setStreamingText(null);  // remove streaming bubble
    }
  }, [isLoading]);

  const addProactive = useCallback((content: string) => {
    const msg: ChatMessage = {
      id: nextId(), role: 'proactive', content,
      timestamp: new Date().toISOString(),
    };
    setMessages(prev => {
      const next = [...prev, msg];
      saveHistory(next);
      return next;
    });
  }, []);

  const clearHistory = useCallback(() => {
    setMessages([]);
    localStorage.removeItem(HISTORY_KEY);
  }, []);

  return { messages, streamingText, isLoading, error, sendMessage, addProactive, clearHistory, ttsEnabled, setTtsEnabled };
}
