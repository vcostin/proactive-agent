import { invoke } from '@tauri-apps/api/core';
import { useCallback, useState } from 'react';
import { ChatMessage } from '../types';

let msgCounter = 0;
function nextId() { return `msg-${++msgCounter}`; }

export function useChat() {
  const [messages, setMessages] = useState<ChatMessage[]>([]);
  const [isLoading, setIsLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const sendMessage = useCallback(async (text: string) => {
    const trimmed = text.trim();
    if (!trimmed || isLoading) return;

    const userMsg: ChatMessage = {
      id: nextId(),
      role: 'user',
      content: trimmed,
      timestamp: new Date().toISOString(),
    };
    setMessages(prev => [...prev, userMsg]);
    setIsLoading(true);
    setError(null);

    try {
      const response = await invoke<string>('send_message', { message: trimmed });
      const assistantMsg: ChatMessage = {
        id: nextId(),
        role: 'assistant',
        content: response,
        timestamp: new Date().toISOString(),
      };
      setMessages(prev => [...prev, assistantMsg]);
    } catch (e) {
      setError(String(e));
    } finally {
      setIsLoading(false);
    }
  }, [isLoading]);

  const addProactive = useCallback((content: string) => {
    setMessages(prev => [
      ...prev,
      { id: nextId(), role: 'proactive', content, timestamp: new Date().toISOString() },
    ]);
  }, []);

  return { messages, isLoading, error, sendMessage, addProactive };
}
