import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { useCallback, useEffect, useRef, useState } from 'react';
import { CuratedPiperVoice, DownloadProgress } from '../../types';

/**
 * Voice list adjacent to Voice output mute.
 * Select persists `tts_voice_id`; download does not change selection until Select.
 * Mute stays independent (caller owns mute toggle).
 */
export function VoicePicker() {
  const [open, setOpen] = useState(false);
  const [voices, setVoices] = useState<CuratedPiperVoice[]>([]);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [busyId, setBusyId] = useState<string | null>(null);
  const [progress, setProgress] = useState<DownloadProgress | null>(null);
  const [error, setError] = useState<string | null>(null);
  const rootRef = useRef<HTMLDivElement>(null);

  const refresh = useCallback(async () => {
    const [catalog, current] = await Promise.all([
      invoke<CuratedPiperVoice[]>('list_curated_voices'),
      invoke<string>('get_tts_voice'),
    ]);
    setVoices(catalog);
    setSelectedId(current);
  }, []);

  useEffect(() => {
    refresh().catch(e => setError(String(e)));
  }, [refresh]);

  useEffect(() => {
    if (!open) return;
    const onDoc = (e: MouseEvent) => {
      if (!rootRef.current?.contains(e.target as Node)) setOpen(false);
    };
    document.addEventListener('mousedown', onDoc);
    return () => document.removeEventListener('mousedown', onDoc);
  }, [open]);

  useEffect(() => {
    let unlisten: (() => void) | undefined;
    listen<DownloadProgress>('download_progress', e => {
      const p = e.payload;
      if (!p.voice_id) return;
      setProgress(p);
    }).then(fn => { unlisten = fn; });
    return () => { unlisten?.(); };
  }, []);

  const selectedLabel =
    voices.find(v => v.id === selectedId)?.label
    ?? (selectedId ? selectedId : 'Voice');

  const handleSelect = async (id: string) => {
    setError(null);
    try {
      await invoke('set_tts_voice', { voiceId: id });
      setSelectedId(id);
    } catch (e) {
      setError(String(e));
    }
  };

  const handleDownload = async (id: string) => {
    setError(null);
    setBusyId(id);
    setProgress(null);
    try {
      await invoke('download_curated_voice', { voiceId: id });
      await refresh();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusyId(null);
      setProgress(null);
    }
  };

  const progressPct = progress && progress.total > 0
    ? Math.min(100, Math.round((100 * progress.downloaded) / progress.total))
    : null;

  return (
    <div ref={rootRef} style={{ position: 'relative' }}>
      <button
        type="button"
        onClick={() => {
          setOpen(v => !v);
          if (!open) refresh().catch(e => setError(String(e)));
        }}
        style={{
          padding: '3px 10px', fontSize: 11,
          borderColor: open ? 'var(--accent)' : 'var(--border)',
          color: open ? 'var(--accent)' : 'var(--text-muted)',
          maxWidth: 120,
          overflow: 'hidden',
          textOverflow: 'ellipsis',
          whiteSpace: 'nowrap',
        }}
        title="Choose Voice output voice"
      >
        {selectedLabel}
      </button>

      {open && (
        <div
          role="listbox"
          aria-label="Curated Piper voices"
          style={{
            position: 'absolute',
            top: '100%',
            right: 0,
            marginTop: 6,
            zIndex: 40,
            width: 280,
            maxHeight: 320,
            overflow: 'auto',
            background: 'var(--bg-panel)',
            border: '1px solid var(--border)',
            borderRadius: 'var(--radius)',
            boxShadow: '0 8px 24px rgba(0,0,0,0.35)',
            padding: 8,
            display: 'flex',
            flexDirection: 'column',
            gap: 6,
          }}
        >
          <div style={{ fontSize: 10, color: 'var(--text-muted)', padding: '0 4px 2px' }}>
            Voice output — pick an installed voice
          </div>

          {voices.map(v => {
            const isSelected = v.id === selectedId;
            const isBusy = busyId === v.id;
            return (
              <div
                key={v.id}
                style={{
                  display: 'flex',
                  alignItems: 'center',
                  gap: 8,
                  padding: '6px 8px',
                  borderRadius: 'var(--radius)',
                  background: isSelected ? 'rgba(100, 160, 255, 0.08)' : 'transparent',
                  border: `1px solid ${isSelected ? 'var(--accent)' : 'transparent'}`,
                }}
              >
                <div style={{ flex: 1, minWidth: 0 }}>
                  <div style={{ fontSize: 12, color: 'var(--text)' }}>
                    {v.label}
                    <span style={{ color: 'var(--text-muted)', marginLeft: 6 }}>{v.locale}</span>
                  </div>
                  <div style={{ fontSize: 10, color: 'var(--text-muted)' }}>
                    {v.installed
                      ? (isSelected ? 'Installed · selected' : 'Installed')
                      : (isSelected ? 'Missing files — re-download' : 'Available')}
                    {isBusy && progressPct != null ? ` · ${progressPct}%` : isBusy ? ' · downloading…' : ''}
                  </div>
                </div>
                {v.installed ? (
                  <button
                    type="button"
                    disabled={isSelected || busyId != null}
                    onClick={() => handleSelect(v.id)}
                    style={{
                      fontSize: 11,
                      padding: '2px 8px',
                      color: isSelected ? 'var(--success)' : 'var(--text)',
                      borderColor: isSelected ? 'var(--success)' : 'var(--border)',
                      opacity: isSelected ? 1 : undefined,
                    }}
                  >
                    {isSelected ? 'Selected' : 'Select'}
                  </button>
                ) : (
                  <button
                    type="button"
                    disabled={busyId != null}
                    onClick={() => handleDownload(v.id)}
                    style={{
                      fontSize: 11,
                      padding: '2px 8px',
                      color: isBusy ? 'var(--accent)' : 'var(--text)',
                      borderColor: isBusy ? 'var(--accent)' : 'var(--border)',
                    }}
                  >
                    {isBusy ? '…' : 'Download'}
                  </button>
                )}
              </div>
            );
          })}

          {error && (
            <div style={{ fontSize: 11, color: 'var(--error)', padding: '4px 4px 0' }}>
              {error}
            </div>
          )}
        </div>
      )}
    </div>
  );
}
