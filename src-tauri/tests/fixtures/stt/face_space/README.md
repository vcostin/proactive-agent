# Face / space STT fixtures (quality follow-up)

Raw mic captures for short isolated words. Diction is fine — the model
knows `▁face` / `▁Face` in `vocab.txt`. Failures here are preprocessing /
greedy-decode fragility, not “you mumbled.”

| File | Capture | STT contract (mono @ 16 kHz) |
|------|---------|------------------------------|
| `face.wav` | 48 kHz stereo PCM | `face_16k.f32` |
| `space.wav` | 48 kHz stereo PCM | `space_16k.f32` |

## Intended text

- `face` → `Face.` (or `Face` / `face`)
- `space` → `Space.`

## What we measured (Host STT `ort`, greedy TDT, 2026-07-17)

Same `face_16k.f32` clip:

| Input | Transcript |
|-------|------------|
| raw (~2.9s, long silence pads, peak ~0.18) | *(empty)* |
| live path peak-norm to 0.7 | `Face.` |
| energy-trim speech island only | `Face.` |
| VAD-only frames (no silence) + peak 0.7 | sometimes `Fez.` |

`space` is more stable on raw (`Space.`), which made the empty `face`
baseline look like a word-specific blind spot. It was mostly **silence
padding + level**, not missing vocabulary.

Live path now: **VAD pre-roll/hangover (200 ms)** → louder-channel mono → rubato
resample → **energy trim (±200 ms)** → peak-norm 0.7 → STT → clean (≥2 letters).
Decoder phrase boosting remains the longer-term fix for residual short-word
confusions; see `.scratch/host-stt-ort-cutover/research-sound-conversion.md`.
