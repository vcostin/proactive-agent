import { invoke } from '@tauri-apps/api/core';
import { useEffect, useRef, useState } from 'react';

/** Polls the live mic energy level (0–1) at 50ms intervals while listening. */
export function useAudioEnergy(listening: boolean) {
  const [energy, setEnergy] = useState(0);
  const frameRef = useRef<number>(0);

  useEffect(() => {
    if (!listening) { setEnergy(0); return; }

    let active = true;
    const poll = async () => {
      if (!active) return;
      try {
        const e = await invoke<number>('get_audio_energy');
        setEnergy(e);
      } catch { /* ignore */ }
      frameRef.current = window.setTimeout(poll, 50);
    };
    poll();

    return () => {
      active = false;
      clearTimeout(frameRef.current);
      setEnergy(0);
    };
  }, [listening]);

  return energy;
}
