Status: ready-for-agent

# Spec: TTS voice options

Source map: `.scratch/tts-voice-options/map.md`  
Catalog research: `.scratch/tts-voice-options/research-piper-voice-catalog.md` / [Piper voice catalog for a curated shortlist](issues/01-piper-voice-catalog-research.md)

**Seams (for `/to-tickets` / `/implement`):**
1. **Primary — Piper voice catalog + selection:** curated list metadata, installed detection (onnx+json), download-on-select, resolve selected voice for speak/preview.
2. **Secondary — config persistence:** selected voice id in app config; missing files fall back safely.

Speak/preview must both go through the same voice resolution path (no second hard-coded lessac-only finder).

## Problem Statement

Voice output in Proactive Agent is only an on/off toggle. Spoken replies and proactive nudges always use a single hardcoded Piper voice (`en_US-lessac-medium`). Users cannot preview or switch among other offline Piper voices, even though many exist and several have clearer permissive licenses than the current default’s source-data terms.

## Solution

Offer a **curated** set of Piper voices: download a voice when the user selects it, preview with a fixed sample sentence, persist the choice, and use that voice for all `speak_text` paths (chat replies and proactive nudges). Stay on Piper — no new TTS engine. Setup Wizard Host completion bar still does not require TTS.

**Done when:** user can pick a non-default installed voice, preview it, and hear chat/proactive speech in that voice after restart.

## User Stories

1. As a user, I want to see which Piper voices are available in the curated list, so that I know what I can install.
2. As a user, I want to see which curated voices are already installed, so that I do not re-download blindly.
3. As a user, I want to download a curated voice when I choose it, so that the app stays small until I need another voice.
4. As a user, I want download progress or a clear busy state while a voice fetches, so that I know the app is working.
5. As a user, I want a clear error if a voice download fails, so that I can retry or pick another voice.
6. As a user, I want to preview a voice with a fixed sample sentence, so that I can hear it before committing.
7. As a user, I want preview to work for an already-installed voice without re-downloading, so that switching is cheap.
8. As a user, I want preview of a not-yet-installed voice to download first (or prompt to download), so that preview matches the real voice.
9. As a user, I want to select an installed voice as my Voice output voice, so that replies use that persona.
10. As a user, I want my voice choice to survive restart, so that I do not re-select every session.
11. As a user, I want Voice output mute to still silence speech, so that selection and mute stay independent.
12. As a user, I want chat replies spoken in my selected voice when Voice output is on, so that conversation matches my choice.
13. As a user, I want proactive nudges spoken in my selected voice when Voice output is on, so that follow-ups match chat.
14. As a user, I want the default voice to remain `en_US-lessac-medium` until I change it, so that existing installs keep working.
15. As a user, I want a male US alternative (joe), so that I am not stuck with one timbre.
16. As a user, I want a female US alternative (kristin), so that I have a clear contrast.
17. As a user, I want another US male (bryce), so that the shortlist has more than one male option.
18. As a user, I want sam as an Apache-licensed option, so that I have a distinct persona with a clear licence.
19. As a user, I want one UK English option (cori), so that en_GB is represented without a huge catalog.
20. As a user, I do not want NC-licensed voices in the picker, so that the product shortlist stays redistribution-safe.
21. As a user, I do not want to browse the full Hugging Face voice tree in-app, so that the UI stays simple.
22. As a developer, I want every voice install to include both `.onnx` and `.onnx.json`, so that Piper does not fail on missing config.
23. As a developer, I want the default voice artifact path to also ensure the sibling `.onnx.json` exists, so that lessac is not a special broken case.
24. As a developer, I want curated voice ids pinned to Hugging Face `v1.0.0` URLs, so that downloads are reproducible.
25. As a developer, I want medium quality only in P0, so that CPU size/latency stay in the ~63 MB class.
26. As a developer, I want voice files under the existing models/tts app-managed location, so that paths stay consistent with today.
27. As a maintainer, I want Setup Wizard to keep shipping/repairing the default voice only for P0, so that wizard scope does not explode into a full voice store.
28. As a maintainer, I want curated extras to be download-on-select from Settings/Voice, so that optional voices are not Host completion gates.
29. As a user, I want the voice picker near the Voice output mute control, so that speech settings live together.
30. As a user, I want each list row to show a human-readable label (locale + name), so that ids are not the only UI text.
31. As a user, I want a Preview action per voice, so that listening is explicit.
32. As a user, I want selecting a voice to apply on the next spoken utterance, so that I do not restart the app to hear the change.
33. As a user, if my configured voice files are missing after restart, I want a safe fallback to the default voice, so that TTS does not hard-fail silently forever.
34. As a user, I want to be told when fallback happened (debug or light UI hint), so that I can re-download.
35. As a QA user, I want to install joe, preview it, select it, mute off, and hear a chat reply in joe, so that the done-when path is testable.
36. As a QA user, I want to quit and reopen and still hear joe, so that persistence is testable.
37. As a QA user, I want cancel/fail of download to leave the previous selection intact, so that a failed fetch does not corrupt config.
38. As a maintainer, I want Core agent chat to work when Piper/voices are missing, so that TTS remains optional relative to Core agent.
39. As a maintainer, I want TTS to stay CPU-side, so that GPU/VRAM remains for the chat model.
40. As a legal-aware maintainer, I want the Lessac research-licence caveat documented in the spec notes, so that commercial distribution risk is not forgotten.
41. As an agent implementing this, I want one resolution path for “which onnx to pass to Piper,” so that speak and preview cannot drift.
42. As an agent, I want catalog metadata (id, label, locale, HF paths, ~size) as data—not hardcoded UI-only strings—so that tests can assert the curated set.
43. As a user, I do not want multi-speaker picker UI in P0, so that speaker_id complexity stays out.
44. As a user, I do not want high-quality variants in P0, so that downloads stay ~63 MB class.
45. As a user, starting a new preview while another preview/speak is playing should replace or stop the previous playback, so that samples do not overlap unintelligibly.

## Implementation Decisions

- **Engine:** Piper only. No Kokoro/cloud/engine swap.
- **Curated P0 list (voice ids):**
  - **Default:** `en_US-lessac-medium` (continuity; document Blizzard research-licence caveat)
  - **Also curated:** `en_US-joe-medium`, `en_US-kristin-medium`, `en_US-bryce-medium`, `en_US-sam-medium`, `en_GB-cori-medium`
  - **Explicitly out of shortlist:** NC voices (ryan, hfc, semaine, …); high variants; multi-speaker packs; jenny_dioco (attribution UX deferred); john/norman/ljspeech/alba (kept off to limit size—alba loses to cori for single en_GB slot)
- **Quality:** medium only for P0.
- **Pairing:** every install/download must place `{id}.onnx` and `{id}.onnx.json` together; speak/preview must resolve both.
- **Pin:** Hugging Face `rhasspy/piper-voices` tag `v1.0.0` URL pattern from research.
- **Storage:** app-managed `models/tts/` (same area as today’s default); installed = both files present for that id.
- **Acquisition:** download-on-select for non-default curated voices; Setup Wizard / repair continues to ensure **default** voice (+ json) only — not the full curated set as wizard steps.
- **Config:** persist stable **voice id** string (not raw filesystem path). Missing/corrupt selection → fall back to default id and use default files if present.
- **Apply timing:** selection affects the next `speak_text` / preview immediately (no app restart).
- **UI:** Settings/Voice area adjacent to Voice output mute; list of curated voices with installed/available state, Select, Preview, and Download when needed.
- **Preview:** fixed sample sentence (product copy, e.g. “Hello, I am your proactive assistant. Voice synthesis is working.” — may reuse existing debug sample). Preview uses the same Piper speak path with the target voice id.
- **Playback overlap:** starting preview (or speak) cancels/replaces in-flight Piper playback rather than mixing.
- **Download UX:** show in-progress state; on failure keep prior selection and surface an error; do not mark installed unless both files verified.
- **Primary seam:** curated catalog + install/detect + resolve voice for speak/preview.
- **Secondary seam:** config field for selected voice id + fallback rules.
- **Vocabulary:** Voice, TTS, Core agent, Setup Wizard, app-managed artifact — per `CONTEXT.md`. TTS remains WIP architecturally; this work only expands Piper voice choice.

## Testing Decisions

- **Good tests** assert external behaviour at the catalog/selection seam: given fixture voice files (or temp dirs), installed detection, resolve-selected-voice returns expected model paths, missing selection falls back to default, curated id set matches the locked list. Do not assert React layout pixels as the primary suite.
- **Modules under test:** voice catalog/metadata; install/detect (onnx+json); voice resolution used by speak/preview; config load/save of voice id (if pure enough).
- **Prior art:** Rust `#[cfg(test)]` under audio/TTS cleaners and scheduler persist tests; Deno tests under `src/chat/` for delivery helpers. Prefer Rust seam tests for catalog/resolve; light Deno/UI only if a pure helper is extracted.
- **Regression bar:** Voice mute still suppresses speech; Core agent works without Piper; default lessac path still speaks when files present; proactive + chat both honor selected voice when mute is off.
- **Hands-on:** download joe → preview → select → unmute → chat reply in joe → restart → still joe.

## Out of Scope

- New TTS engine or architecture swap
- In-app browse of the full Hugging Face piper-voices tree
- Per-language auto voice routing
- Cloud / online synthesis
- Changing STT or Host STT path
- Requiring TTS for Core agent / Setup Wizard Host completion bar
- High-quality and multi-speaker voices in P0
- NC-licensed voices in the curated list
- Per-voice custom preview text editor
- Commercial licence remediation beyond documenting the Lessac caveat (e.g. forcibly removing lessac as default)

## Further Notes

- Research asset: `.scratch/tts-voice-options/research-piper-voice-catalog.md`.
- Open map fog at publish time (acceptable defaults locked above): finer download progress chrome; wizard listing curated optionals (deferred — wizard stays default-only); playback cancel details may be refined in implementation.
- Wayfinder grilling tickets 02–05 may still be open — this spec is the collapsed authority for build; update those tickets to point here if desired.
- `CONTEXT.md` still marks TTS as WIP; optional follow-up `/domain-modeling` to add “Piper voice” / “voice id” glossary terms.
