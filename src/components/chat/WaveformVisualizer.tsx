import { useEffect, useRef } from 'react';

interface Props {
  isActive: boolean;
  energyLevel?: number; // 0-1
  width?: number;
  height?: number;
}

const BAR_COUNT = 24;

export function WaveformVisualizer({ isActive, energyLevel = 0, width = 120, height = 28 }: Props) {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const frameRef = useRef<number>(0);
  const barsRef = useRef<Float32Array>(new Float32Array(BAR_COUNT));

  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;
    const ctx = canvas.getContext('2d')!;
    const bars = barsRef.current;

    const draw = () => {
      ctx.clearRect(0, 0, width, height);

      const target = isActive ? energyLevel : 0;
      const barW = width / BAR_COUNT;
      const gap = 1;

      for (let i = 0; i < BAR_COUNT; i++) {
        // Decay bars toward target with a bit of random jitter when active
        const jitter = isActive ? (Math.random() - 0.5) * 0.4 * target : 0;
        bars[i] = bars[i] * 0.85 + (target + jitter) * 0.15;
        bars[i] = Math.max(0, Math.min(1, bars[i]));

        const h = Math.max(2, bars[i] * height);
        const x = i * barW + gap / 2;
        const y = (height - h) / 2;

        const alpha = isActive ? 0.5 + bars[i] * 0.5 : 0.25;
        ctx.fillStyle = `rgba(74, 158, 255, ${alpha})`;
        ctx.fillRect(x, y, barW - gap, h);
      }

      frameRef.current = requestAnimationFrame(draw);
    };

    frameRef.current = requestAnimationFrame(draw);
    return () => cancelAnimationFrame(frameRef.current);
  }, [isActive, energyLevel, width, height]);

  return (
    <canvas
      ref={canvasRef}
      width={width}
      height={height}
      style={{ borderRadius: 4, opacity: isActive ? 1 : 0.4 }}
    />
  );
}
