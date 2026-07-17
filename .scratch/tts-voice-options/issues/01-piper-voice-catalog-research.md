# Piper voice catalog for a curated shortlist

Type: research
Status: resolved
Blocked by:

## Question

Which Piper voices (from primary sources — rhasspy/piper-voices / Piper docs) are practical candidates for a small curated offline CPU shortlist for this product: quality tier, approximate download size, language/locale, license, and any pairing requirements (`.onnx` + `.onnx.json`)? Prefer facts that let us pick ~3–8 voices including keeping `en_US-lessac-medium` as default.

## Answer

Every voice needs `.onnx` + `.onnx.json` (~63 MB medium / ~114 MB high for single-speaker English). Keep `en_US-lessac-medium` as default; strong add-ons are joe (CC0), kristin/bryce/john (public domain), sam (Apache-2.0), and cori/alba/jenny_dioco (en_GB). Avoid NC voices (ryan, hfc, semaine); Lessac’s Blizzard source data is research-only for commercial synthesis — legal caveat.

Full write-up: [research-piper-voice-catalog.md](../research-piper-voice-catalog.md)
