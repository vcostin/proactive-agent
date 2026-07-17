# TTS voice options

Labels: `wayfinder:map`

## Destination

A thin, implementable **spec** for choosing among **curated Piper voices** (download-on-select, persist selection, **voice preview** with a fixed sample sentence). Done when: user can pick a non-default installed voice, preview it, and hear chat/proactive speech in that voice after restart — still Piper, not a new TTS engine.

## Notes

- Domain: Voice / TTS (WIP per `CONTEXT.md`). Consult `CONTEXT.md` Voice section, `src-tauri/src/audio/tts.rs`, `constants::TTS_MODEL_FILE` (`en_US-lessac-medium`), Platform-module piper/voice artifact defs.
- Skills: `/grilling`, `/domain-modeling`; `/prototype` only if a ticket asks for UI fidelity.
- Plan, don't build — produce decisions, then the handoff spec.
- Breadth lock: curated Piper list (not full HF dump); download-on-select; picker in Settings/Voice near the mute control; persist voice id in config; preview = fixed sample sentence; Piper-only.
- Today: Voice output is on/off only; `find_tts_model()` always resolves the single lessac medium ONNX.

## Decisions so far

- [Piper voice catalog for a curated shortlist](issues/01-piper-voice-catalog-research.md) — Keep lessac-medium; pair every voice with onnx+json (~63 MB medium); prefer joe/kristin/bryce/sam + en_GB cori/alba; avoid NC; Lessac research-licence caveat.

## Not yet specified

- Download progress / failure UX while fetching a voice.
- Whether Setup Wizard should still ship/offer the default voice only, or list curated voices as optional artifacts.
- How preview shares or serializes with in-flight `speak_text` playback.

## Out of scope

- New TTS engine or architecture swap (Kokoro, cloud TTS, etc.)
- Browsing the full Hugging Face piper-voices tree in-app
- Per-language auto voice routing
- Cloud / online synthesis
- Changing STT or Host STT path
- Requiring TTS for Core agent / Setup Wizard Host completion bar
