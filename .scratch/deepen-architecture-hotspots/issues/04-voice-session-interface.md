# VoiceSession capture lifecycle interface

Type: grilling
Status: resolved
Blocked by:

## Question

What is the external **interface** of a deepened `VoiceSession` module for Host mic capture lifecycle — `start` / `stop` (and Drop), soft-fail when STT is unavailable, injected energy/transcript sinks — such that Tauri commands stop owning thread spawn, `mpsc`, and stop-flag wiring?

Confirm: delete or absorb the unused `start_capture` / `VoiceHandle` path. Explicitly leave UtterancePipeline (Tauri-free loop / dual VAD) out of this interface.

## Answer

Locked in [spec.md](../spec.md) Implementation Decisions → VoiceSession: start/stop/Drop with optional STT + sinks; absorb thread/channel/soft-fail; delete unused capture handle; UtterancePipeline out of scope.
