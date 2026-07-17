# Research: Piper voice catalog for a curated offline shortlist

**Date:** 2026-07-17  
**Scope:** Practical en_US / en_GB Piper voices for a small curated CPU shortlist (~3–8), keeping `en_US-lessac-medium` as default.  
**Sources:** rhasspy/piper (and successor OHF-Voice/piper1-gpl), Hugging Face `rhasspy/piper-voices` (tag `v1.0.0`), per-voice `MODEL_CARD`s, and linked dataset licenses.

---

## Summary

Every Piper voice needs **two files**: `{id}.onnx` plus `{id}.onnx.json` (config defaults to model path + `.json`). English voices in `v1.0.0` are tagged **low / medium / high** (plus **x_low** elsewhere in the catalog); TRAINING maps those tiers to sample rate and model size. Single-speaker **medium** ONNX files cluster around **~63 MB** (+ ~5 KB JSON); **high** is roughly **~114–121 MB**.

This repo already ships **`en_US-lessac-medium`** as the only TTS model (`constants::TTS_MODEL_FILE`, Linux artifact URL on HF `v1.0.0`). For a curated picker, prefer **single-speaker medium** voices with clear permissive dataset licenses; treat **NC** and **Lessac Blizzard research** terms as shortlist risks. A workable P0-shaped set is: keep **lessac-medium** as default, then add **joe**, **kristin**, **bryce** / **john**, **sam**, plus **cori** or **alba** / **jenny_dioco** for en_GB.

---

## This repo’s default today

| Fact | Detail |
|------|--------|
| Voice id | `en_US-lessac-medium` |
| Filename constant | `en_US-lessac-medium.onnx` in `src-tauri/src/constants.rs` (`TTS_MODEL_FILE`) |
| Download URL (Linux artifact) | `https://huggingface.co/rhasspy/piper-voices/resolve/v1.0.0/en/en_US/lessac/medium/en_US-lessac-medium.onnx` in `src-tauri/src/platform/linux.rs` (`id: "tts-voice"`) |
| Resolver | `find_tts_model()` always looks for that single filename under `models/tts/` |

**Gap vs Piper pairing rule:** the platform artifact currently downloads **only the `.onnx`**, not the sibling `.onnx.json`. Piper’s C++ CLI defaults config to `modelPath + ".json"` and errors if missing. Sources: [piper README (historical)](https://raw.githubusercontent.com/rhasspy/piper/9b1c6397/README.md); [OHF VOICES.md](https://github.com/OHF-Voice/piper1-gpl/blob/main/docs/VOICES.md); [piper `main.cpp` config default](https://github.com/rhasspy/piper/blob/9b1c6397/src/cpp/main.cpp) (`default: model path + .json`).

---

## Pairing requirement (every voice)

Official docs state you need:

1. `.onnx` model — e.g. `en_US-lessac-medium.onnx`
2. `.onnx.json` config — e.g. `en_US-lessac-medium.onnx.json`

URL pattern (HF tag `v1.0.0`):

```text
https://huggingface.co/rhasspy/piper-voices/resolve/v1.0.0/{lang_family}/{lang_code}/{dataset}/{quality}/{lang_code}-{dataset}-{quality}.onnx
https://huggingface.co/rhasspy/piper-voices/resolve/v1.0.0/{lang_family}/{lang_code}/{dataset}/{quality}/{lang_code}-{dataset}-{quality}.onnx.json
```

Confirmed by:

- [OHF-Voice/piper1-gpl docs/VOICES.md](https://github.com/OHF-Voice/piper1-gpl/blob/main/docs/VOICES.md)
- [rhasspy/piper README @ 9b1c6397](https://raw.githubusercontent.com/rhasspy/piper/9b1c6397/README.md)
- [rhasspy/piper `script/generate_voices_md.py`](https://github.com/rhasspy/piper/blob/9b1c6397/script/generate_voices_md.py) (same URL formats)
- HF tree / [`voices.json`](https://huggingface.co/rhasspy/piper-voices/raw/v1.0.0/voices.json) lists both files per voice

Multi-speaker voices also need a **speaker** / **speaker_id** at runtime (`--speaker` / JSON `speaker_id`). Source: same piper README “Usage” / “JSON Input” sections.

---

## Quality tiers

Documented quality labels used in the voice catalog / tooling:

| Tier | Documented meaning (TRAINING.md) | Notes |
|------|----------------------------------|-------|
| **x_low** | Not defined in TRAINING.md quality bullet list | Appears in `_QUALITY` map in `generate_voices_md.py` and in some non-English `VOICES.md` entries; **no en_US/en_GB x_low** in `v1.0.0` `voices.json` |
| **low** | 16,000 Hz, smaller voice model | en examples: `lessac/low`, `amy/low`, `ryan/low`, `southern_english_female/low` |
| **medium** | 22,050 Hz, smaller voice model | Default product tier; most single-speaker English voices |
| **high** | 22,050 Hz, **larger** voice model; TRAINING: “sounds better, but is much slower” | e.g. `lessac/high`, `ryan/high`, `cori/high`, `ljspeech/high`, `libritts/high` |

Sources:

- [rhasspy/piper TRAINING.md](https://github.com/rhasspy/piper/blob/master/TRAINING.md) (quality bullets + `--quality high` note)
- [generate_voices_md.py `_QUALITY`](https://github.com/rhasspy/piper/blob/9b1c6397/script/generate_voices_md.py) (`x_low`, `low`, `medium`, `high`)
- Per-voice `MODEL_CARD` “Quality” / “Samplerate” fields on HF

For offline **CPU** shortlists, **medium** is the practical default: same ~63 MB class as current lessac, 22.05 kHz (matches this app’s piper→device resample assumptions). Prefer **high** only if latency budget allows (~2× ONNX size).

---

## License layers

1. **Hugging Face model card** for `rhasspy/piper-voices` declares package metadata `license: mit`.  
   Source: [HF README / cardData](https://huggingface.co/rhasspy/piper-voices).
2. **Piper docs** say Piper does not add further restrictions, but **`MODEL_CARD` per voice has important licensing** — review carefully.  
   Sources: [OHF VOICES.md](https://github.com/OHF-Voice/piper1-gpl/blob/main/docs/VOICES.md); [piper README @ 9b1c6397](https://raw.githubusercontent.com/rhasspy/piper/9b1c6397/README.md).
3. **Dataset / MODEL_CARD terms** are what matter for redistributing a curated list in a product.

### Lessac (current default) — research-data caveat

`MODEL_CARD` for `en_US-lessac-*` points at the Blizzard 2013 Lessac license page. That agreement defines **“Research Purposes”** and **explicitly excludes commercial** development / marketing / sale of voice synthesis products or services.

- MODEL_CARD: <https://huggingface.co/rhasspy/piper-voices/raw/v1.0.0/en/en_US/lessac/medium/MODEL_CARD>
- License page: <https://www.cstr.ed.ac.uk/projects/blizzard/2013/lessac_blizzard2013/license.html>

**Implication for product:** keeping lessac as the **UX default** matches the shipped binary today, but legal should treat the **source-data research licence** as a risk if the app is commercial. Prefer shipping additional shortlist voices with clearer permissive MODEL_CARDs (CC0 / public domain / Apache / CC BY) for user-selectable alternatives.

---

## Size ballparks (`v1.0.0` HF file sizes)

Measured via Hugging Face tree API for `rhasspy/piper-voices` tag `v1.0.0` (ONNX bytes; JSON is ~4–20 KB).

| Pattern | Approx. ONNX size | Typical JSON |
|---------|-------------------|--------------|
| Single-speaker **low/medium** | **~63.1–63.5 MB** | ~4–5 KB |
| Single-speaker **high** | **~114–121 MB** | ~5 KB |
| Multi-speaker medium (arctic, aru, semaine, vctk, …) | **~77–79 MB** | ~5–7 KB |
| Multi-speaker high (`libritts`) | **~137 MB** | ~20 KB |

Exact examples from `voices.json` `size_bytes` / tree API:

| Voice id | Quality | Speakers | ONNX bytes | JSON bytes |
|----------|---------|----------|------------|------------|
| `en_US-lessac-medium` | medium | 1 | 63,201,294 | 4,885 |
| `en_US-joe-medium` | medium | 1 | 63,201,294 | 4,794 |
| `en_US-amy-medium` | medium | 1 | 63,201,294 | 4,882 |
| `en_US-kristin-medium` | medium | 1 | 63,531,379 | 4,968 |
| `en_US-bryce-medium` | medium | 1 | 63,531,379 | 4,966 |
| `en_US-sam-medium` | medium | 1 | 62,950,044 | 5,040 |
| `en_US-lessac-high` | high | 1 | ~113.9 MB | ~4.9 KB |
| `en_GB-cori-medium` | medium | 1 | 63,531,379 | 4,966 |
| `en_GB-cori-high` | high | 1 | ~114.2 MB | ~5.0 KB |
| `en_GB-alba-medium` | medium | 1 | 63,201,294 | 4,888 |
| `en_US-libritts-high` | high | 904 | ~136.7 MB | ~20 KB |

Download cost per installable voice ≈ **ONNX + JSON** (JSON negligible). Curating 6 medium voices ≈ **~380 MB** total if all installed; download-on-select keeps baseline at one ~63 MB voice.

---

## English catalog snapshot (`v1.0.0`)

Full listing: [rhasspy/piper VOICES.md](https://github.com/rhasspy/piper/blob/master/VOICES.md) and HF tree under `en/en_US`, `en/en_GB`. Metadata index: [`voices.json`](https://huggingface.co/rhasspy/piper-voices/raw/v1.0.0/voices.json) (37 en_* keys).

### Strong single-speaker candidates (practical for shortlist)

| Voice id | Locale | Quality | ~ONNX | Speakers | MODEL_CARD license (dataset) | Notes |
|----------|--------|---------|-------|----------|------------------------------|-------|
| **`en_US-lessac-medium`** | en_US | medium | ~63.2 MB | 1 | Blizzard Lessac **research** licence (see above) | **Current default**; also low/high siblings |
| `en_US-joe-medium` | en_US | medium | ~63.2 MB | 1 | **CC0** (NabuCasa voice-datasets) | Male; finetuned from lessac medium |
| `en_US-kristin-medium` | en_US | medium | ~63.5 MB | 1 | **public domain** (LibriVox-derived) | Female |
| `en_US-bryce-medium` | en_US | medium | ~63.5 MB | 1 | **public domain** | Male |
| `en_US-john-medium` | en_US | medium | ~63.5 MB | 1 | **public domain** | Male; finetuned from kristin |
| `en_US-norman-medium` | en_US | medium | ~63.5 MB | 1 | **public domain** | Male |
| `en_US-ljspeech-medium` | en_US | medium | ~63.5 MB | 1 | **public domain** (LJ Speech) | Female; high sibling ~114 MB |
| `en_US-sam-medium` | en_US | medium | ~63.0 MB | 1 | **Apache-2.0** | Non-binary voice dataset |
| `en_US-kathleen-low` | en_US | low | ~63.1 MB | 1 | **CC0** | 16 kHz; smaller quality tier |
| `en_GB-cori-medium` | en_GB | medium | ~63.5 MB | 1 | **public domain** | Female UK; high ~114 MB |
| `en_GB-alba-medium` | en_GB | medium | ~63.2 MB | 1 | **CC BY 4.0** | Scottish English corpus |
| `en_GB-jenny_dioco-medium` | en_GB | medium | ~63.2 MB | 1 | Dataset: commercial OK **with attribution “Jenny” / “Jenny (Dioco)”** | Irish-accented GB voice |
| `en_GB-northern_english_male-medium` | en_GB | medium | ~63.2 MB | 1 | **CC-BY-SA 4.0** | ShareAlike on dataset |

MODEL_CARD URLs follow  
`https://huggingface.co/rhasspy/piper-voices/raw/v1.0.0/en/{locale}/{name}/{quality}/MODEL_CARD`.

### Popular but weaker shortlist fits

| Voice id | Issue for curated product list |
|----------|--------------------------------|
| `en_US-ryan-*` | MODEL_CARD: **CC BY-NC-SA 4.0** (non-commercial) |
| `en_US-hfc_female-medium` / `hfc_male-medium` | MODEL_CARD: **CC BY-NC-SA 4.0** |
| `en_GB-semaine-medium` | **CC BY-NC-SA 4.0** + 4 speakers |
| `en_US-amy-*`, `en_GB-alan-*`, `en_US-danny-*`, `en_US-kusal-*` | MODEL_CARD license “See URL” (Mycroft / mimic) — needs extra license chase before shipping |
| `en_US-arctic-medium`, `l2arctic`, `libritts*`, `en_GB-aru`, `vctk` | Multi-speaker; needs speaker UI / default speaker_id |
| `en_US-reza_ibrahim-medium` | MODEL_CARD bilingual fa_IR/en_US; CC0 but niche |

---

## Suggested curated shortlist (~6–8) for this product

Facts-only recommendation shaped for offline CPU + download-on-select (not a product decision):

1. **`en_US-lessac-medium`** — keep as default (already wired); document Blizzard research-licence caveat.
2. **`en_US-joe-medium`** — CC0 male alternative, same size class.
3. **`en_US-kristin-medium`** — public-domain female US.
4. **`en_US-bryce-medium`** or **`en_US-john-medium`** — public-domain male US (pick one to limit list size).
5. **`en_US-sam-medium`** — Apache-2.0; distinct persona.
6. **`en_GB-cori-medium`** — public-domain UK female (or **`en_GB-alba-medium`** if CC BY attribution is fine).
7. Optional 7th/8th: **`en_GB-jenny_dioco-medium`** (attribution UX) and/or **`en_US-ljspeech-medium`**.

Defer **high** variants and multi-speaker packs until latency / speaker UX exist. Defer **NC** voices unless the product is clearly non-commercial.

---

## Implementation notes for a later spec (research only)

- Always download **onnx + onnx.json** to the same directory; Piper resolves `*.onnx.json` automatically.
- HF pin **`v1.0.0`** matches current Linux URL; OHF docs still point at the same HF repo.
- Samples for listening: <https://rhasspy.github.io/piper-samples> (linked from OHF VOICES.md).
- Development of Piper itself moved to [OHF-Voice/piper1-gpl](https://github.com/OHF-Voice/piper1-gpl); voice assets remain on [rhasspy/piper-voices](https://huggingface.co/rhasspy/piper-voices).

---

## Source index

| Claim area | Primary source |
|------------|----------------|
| Two files per voice; lessac example URLs | <https://github.com/OHF-Voice/piper1-gpl/blob/main/docs/VOICES.md>, <https://raw.githubusercontent.com/rhasspy/piper/9b1c6397/README.md> |
| Config path = model + `.json` | <https://github.com/rhasspy/piper/blob/9b1c6397/src/cpp/main.cpp> |
| Quality tiers low/medium/high + sample rates | <https://github.com/rhasspy/piper/blob/master/TRAINING.md> |
| x_low in catalog ordering | <https://github.com/rhasspy/piper/blob/9b1c6397/script/generate_voices_md.py> |
| Voice inventory + download links | <https://github.com/rhasspy/piper/blob/master/VOICES.md>, <https://huggingface.co/rhasspy/piper-voices/tree/v1.0.0> |
| File sizes / speaker counts | HF tree API + <https://huggingface.co/rhasspy/piper-voices/raw/v1.0.0/voices.json> |
| Repo-level MIT metadata | <https://huggingface.co/rhasspy/piper-voices> README YAML `license: mit` |
| Per-voice licences | Each voice’s `MODEL_CARD` under HF `v1.0.0` |
| Lessac research / no-commercial synthesis | <https://www.cstr.ed.ac.uk/projects/blizzard/2013/lessac_blizzard2013/license.html> |
| Jenny attribution / commercial OK | <https://github.com/dioco-group/jenny-tts-dataset> README “License” section |
| This repo default | `src-tauri/src/constants.rs`, `src-tauri/src/platform/linux.rs` |
