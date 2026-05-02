import { AudioState as IAudioState } from '../../types';

interface Props { audio: IAudioState; }

export function AudioState({ audio }: Props) {
  const isActive = audio.vad_state === 'Active';

  return (
    <div style={{ display: 'flex', flexDirection: 'column', gap: 6, fontSize: 12 }}>
      <Row label="device" value={audio.device_name || '—'} />
      <Row label="sample rate" value={audio.sample_rate ? `${audio.sample_rate} Hz` : '—'} />
      <Row label="channels" value={audio.channels ? String(audio.channels) : '—'} />

      <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', gap: 8 }}>
        <span style={{ color: 'var(--text-muted)' }}>VAD</span>
        <div style={{ display: 'flex', alignItems: 'center', gap: 6 }}>
          <div style={{ width: 80, height: 5, background: '#222', borderRadius: 3, overflow: 'hidden' }}>
            <div style={{
              height: '100%',
              width: `${audio.energy_level * 100}%`,
              background: isActive ? 'var(--accent)' : '#555',
              transition: 'width 0.1s',
            }} />
          </div>
          <span style={{ color: isActive ? 'var(--accent)' : 'var(--text-muted)', width: 40 }}>
            {audio.vad_state.toLowerCase()}
          </span>
        </div>
      </div>

      <Row label="last STT" value={audio.last_stt_result ?? '—'} truncate />
      <Row label="STT latency" value={`${audio.last_stt_latency_ms} ms`} highlight={audio.last_stt_latency_ms > 2000} />
      <Row label="TTS latency" value={`${audio.last_tts_latency_ms} ms`} highlight={audio.last_tts_latency_ms > 2000} />
      <Row label="TTS buffer" value={`${audio.tts_buffer_fill_pct.toFixed(0)}%`} />
    </div>
  );
}

function Row({ label, value, highlight = false, truncate = false }: {
  label: string; value: string; highlight?: boolean; truncate?: boolean;
}) {
  return (
    <div style={{ display: 'flex', justifyContent: 'space-between', gap: 8 }}>
      <span style={{ color: 'var(--text-muted)', flexShrink: 0 }}>{label}</span>
      <span style={{
        color: highlight ? '#c47a00' : 'var(--text)',
        overflow: truncate ? 'hidden' : undefined,
        textOverflow: truncate ? 'ellipsis' : undefined,
        whiteSpace: truncate ? 'nowrap' : undefined,
        maxWidth: truncate ? 160 : undefined,
      }}>
        {value}
      </span>
    </div>
  );
}
