# Face / space STT fixtures (quality follow-up)

Raw mic captures for short-word confusions that greedy Parakeet TDT
often mishandles without decoder phrase boosting.

| File | Capture | STT contract (mono @ 16 kHz) |
|------|---------|------------------------------|
| `face.wav` | 48 kHz stereo PCM | `face_16k.f32` |
| `space.wav` | 48 kHz stereo PCM | `space_16k.f32` |

## Intended text

- `face` → `face` (or `Face.` if the model capitalizes)
- `space` → `space` (or `Space.`)

## Observed baseline (Host STT `ort`, greedy TDT, 2026-07-17)

| Clip | Transcript |
|------|------------|
| face | *(empty)* |
| space | `Space.` |

These are **not** Host cutover gates. They are the failing baseline for the
decoder biasing / phrase-boost follow-up once that work starts.
