# voice-core

The pure core of local voice dictation (Whisper), shared by every product that wants speech.
Tauri-free and engine-free: nothing here opens a microphone or loads a model, so all of it is
unit-tested.

| Module | What it holds |
|---|---|
| `models` | The ggml model catalogue (names, sizes and SHA-256 digests pinned from the official `ggerganov/whisper.cpp` repository), the per-model cost (calibrated on the real encoder timings), the real-time-factor estimate, the hardware recommendation (measurements override estimates) and the "is this comfortable" verdict |
| `audio` | Downmix, resample to 16 kHz, RMS and the 0..1 meter mapping |
| `speech` | The adaptive energy gate, the end-of-speech state machine (`Endpointer`), `contains_speech` (the guard against hallucinated text on silence) and the `WakeBudget` rate limit |
| `wake` | Fuzzy wake-phrase matching (edit distance over letters, start of the utterance only) |
| `vocab` | The `initial_prompt` builder from project / file / symbol names |
| `config` | `VoiceConfig` (`<profile>/voice/config.toml`) and the shared models directory |
| `state` | `VoiceState`: event egress, live config (patched, never replaced), the idle clock |
| `plan` | Per-run limits: transcription timeout, shortened encoder window, thread count |
| `selection` | Which model is in use given the config and what is on disk; auto-select after install / fallback after remove |
| `stream` | Live dictation: `SegmentCutter` (cuts at pauses), `Assembler` (joins pieces, context for the next one) |
| `bench` | Measured real-time factor per model, persisted next to the models |

Call sites use `voice_core::prelude::*`. Tests: `cargo test -p voice-core`.
