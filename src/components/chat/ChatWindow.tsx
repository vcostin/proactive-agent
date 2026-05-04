import { KeyboardEvent, useCallback, useEffect, useRef, useState } from 'react';
import { useChat } from '../../hooks/useChat';
import { useLlamaReady } from '../../hooks/useLlamaReady';
import { useProactiveEvents } from '../../hooks/useProactiveEvents';
import { DeferredMessage } from '../../types';
import { WaveformVisualizer } from './WaveformVisualizer';

interface Props {
  modelName: string;
  onModelClick: () => void;
}

export function ChatWindow({ modelName, onModelClick }: Props) {
  const { messages, streamingText, isLoading, error, sendMessage, addProactive, clearHistory } = useChat();
  const llamaReady = useLlamaReady();
  const [input, setInput] = useState('');
  const [listening, setListening] = useState(false);
  const bottomRef = useRef<HTMLDivElement>(null);

  const handleProactive = useCallback(
    (msg: DeferredMessage) => addProactive(msg.message),
    [addProactive],
  );
  useProactiveEvents(handleProactive);

  // Scroll to bottom on new messages
  useEffect(() => {
    bottomRef.current?.scrollIntoView({ behavior: 'smooth' });
  }, [messages]);

  const handleSend = () => {
    sendMessage(input);
    setInput('');
  };

  const handleKey = (e: KeyboardEvent<HTMLTextAreaElement>) => {
    if (e.key === 'Enter' && !e.shiftKey) {
      e.preventDefault();
      handleSend();
    }
  };

  return (
    <div style={{ display: 'flex', flexDirection: 'column', height: '100%' }}>

      {/* ── Header ── */}
      <div style={{
        display: 'flex', alignItems: 'center', gap: 12,
        padding: '8px 14px', borderBottom: '1px solid var(--border)',
        background: 'var(--bg-panel)',
      }}>
        <button
          onClick={onModelClick}
          style={{ fontSize: 11, padding: '3px 8px', opacity: 0.8 }}
          title="Switch model"
        >
          {modelName || 'no model loaded'}
        </button>
        <button
          onClick={() => { if (confirm('Clear conversation history?')) clearHistory(); }}
          style={{ fontSize: 10, padding: '2px 8px', color: 'var(--text-muted)' }}
          title="Clear chat history"
        >
          clear
        </button>
        <div style={{ flex: 1 }} />
        <WaveformVisualizer isActive={listening} energyLevel={listening ? 0.6 : 0} />
        <button
          onClick={() => setListening(l => !l)}
          style={{
            padding: '3px 10px', fontSize: 11,
            borderColor: listening ? 'var(--accent)' : 'var(--border)',
            color: listening ? 'var(--accent)' : 'var(--text-muted)',
          }}
          title="Toggle voice input"
        >
          {listening ? '🎙 listening' : '🎙'}
        </button>
      </div>

      {/* ── Loading banner ── */}
      {!llamaReady && (
        <div style={{
          display: 'flex', alignItems: 'center', gap: 8,
          padding: '6px 14px', background: 'rgba(74,158,255,0.08)',
          borderBottom: '1px solid var(--accent-dim)', fontSize: 11,
          color: 'var(--accent)',
        }}>
          <Spinner /> Loading model — you can type, message will send when ready
        </div>
      )}

      {/* ── Message list ── */}
      <div style={{
        flex: 1, overflowY: 'auto', padding: '14px 16px',
        display: 'flex', flexDirection: 'column', gap: 10,
      }}>
        {messages.length === 0 && (
          <div style={{ margin: 'auto', opacity: 0.3, textAlign: 'center', lineHeight: 2 }}>
            proactive-agent<br />
            <span style={{ fontSize: 11 }}>send a message to begin</span>
          </div>
        )}

        {messages.map(msg => (
          <MessageBubble key={msg.id} role={msg.role} content={msg.content} />
        ))}

        {/* Streaming bubble — shows tokens as they arrive */}
        {streamingText !== null && (
          <div style={{ alignSelf: 'flex-start', maxWidth: '75%' }}>
            <Bubble style={{ background: 'var(--asst-bubble)' }}>
              {streamingText.length > 0
                ? <Markdown text={streamingText} />
                : <DotsLoader />
              }
              <span style={{
                display: 'inline-block', width: 8, height: 12,
                background: 'var(--accent)', marginLeft: 2,
                animation: 'blink 1s step-end infinite',
                verticalAlign: 'text-bottom',
              }} />
            </Bubble>
          </div>
        )}

        {error && (
          <div style={{
            alignSelf: 'center', color: 'var(--error)', fontSize: 11,
            padding: '4px 10px', border: '1px solid var(--error)',
            borderRadius: 'var(--radius)',
          }}>
            {error}
          </div>
        )}

        <div ref={bottomRef} />
      </div>

      {/* ── Input bar ── */}
      <div style={{
        padding: '10px 14px', borderTop: '1px solid var(--border)',
        background: 'var(--bg-panel)', display: 'flex', gap: 8, alignItems: 'flex-end',
      }}>
        <textarea
          value={input}
          onChange={e => setInput(e.target.value)}
          onKeyDown={handleKey}
          placeholder="Type a message… (Enter to send, Shift+Enter for newline)"
          rows={1}
          style={{
            flex: 1, resize: 'none', lineHeight: 1.5,
            minHeight: 36, maxHeight: 160,
          }}
        />
        <button
          className="primary"
          onClick={handleSend}
          disabled={!input.trim() || isLoading || !llamaReady}
          style={{ height: 36, padding: '0 16px' }}
          title={!llamaReady ? 'Waiting for model to load…' : undefined}
        >
          {!llamaReady ? '…' : 'send'}
        </button>
      </div>
    </div>
  );
}

// ── Sub-components ────────────────────────────────────────────────────────────

function MessageBubble({ role, content }: { role: string; content: string }) {
  const isUser = role === 'user';
  const isPro = role === 'proactive';

  if (isPro) {
    return (
      <div style={{ alignSelf: 'center', maxWidth: '80%' }}>
        <Bubble style={{
          background: 'var(--pro-bubble)',
          border: '1px solid var(--pro-border)',
          fontSize: 12,
        }}>
          <span style={{ color: 'var(--pro-border)', marginRight: 6 }}>◈</span>
          {content}
        </Bubble>
      </div>
    );
  }

  return (
    <div style={{ alignSelf: isUser ? 'flex-end' : 'flex-start', maxWidth: '75%' }}>
      <Bubble style={{ background: isUser ? 'var(--user-bubble)' : 'var(--asst-bubble)' }}>
        <Markdown text={content} />
      </Bubble>
    </div>
  );
}

function Bubble({ children, style }: { children: React.ReactNode; style?: React.CSSProperties }) {
  return (
    <div style={{
      padding: '8px 12px',
      borderRadius: 'var(--radius)',
      border: '1px solid var(--border)',
      lineHeight: 1.6,
      whiteSpace: 'pre-wrap',
      wordBreak: 'break-word',
      ...style,
    }}>
      {children}
    </div>
  );
}

/** Minimal inline code / code-block renderer — no markdown library needed for MVP. */
function Markdown({ text }: { text: string }) {
  const parts = text.split(/(```[\s\S]*?```|`[^`]+`)/g);
  return (
    <>
      {parts.map((part, i) => {
        if (part.startsWith('```') && part.endsWith('```')) {
          const code = part.slice(3, -3).replace(/^\w+\n/, '');
          return (
            <pre key={i} style={{
              background: '#0a0a0a', border: '1px solid var(--border)',
              borderRadius: 4, padding: '8px 10px', overflowX: 'auto',
              fontSize: 12, marginTop: 6,
            }}>
              {code}
            </pre>
          );
        }
        if (part.startsWith('`') && part.endsWith('`')) {
          return (
            <code key={i} style={{
              background: '#0a0a0a', padding: '1px 5px',
              borderRadius: 3, fontSize: 12,
            }}>
              {part.slice(1, -1)}
            </code>
          );
        }
        return <span key={i}>{part}</span>;
      })}
    </>
  );
}

function DotsLoader() {
  return (
    <span style={{ letterSpacing: 4, color: 'var(--text-muted)' }}>
      · · ·
    </span>
  );
}

function Spinner() {
  return (
    <span style={{
      display: 'inline-block', width: 10, height: 10,
      border: '1px solid var(--accent-dim)',
      borderTop: '1px solid var(--accent)',
      borderRadius: '50%',
      animation: 'spin 0.8s linear infinite',
      flexShrink: 0,
    }} />
  );
}
