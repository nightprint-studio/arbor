# arbor-nemus-engine

The **timing runtime** of [nemus](../../../design/nemus), Arbor's live-coding music engine. This is
**Fase 3**: it queries the pure patterns and fires sample-accurate triggers at the audio backend, in
time. It depends on `arbor-nemus-pattern` + `arbor-nemus-audio` — and deliberately **not** on
`arbor-nemus-lang`: the shell evaluates `.nemus` source → `Tracks<ControlMap>` and hands those in;
the engine speaks only patterns + the audio seam.

```text
Tracks<ControlMap> ──query look-ahead──▶ VoiceEvents ──AudioSink──▶ audio
```

## Pieces

| Module | Role |
|---|---|
| `clock` | `Epoch` — maps cycle-time ↔ output frames (`frames_per_cycle = sample_rate / cps`). Audio owns the sample clock; the engine owns `cps` and re-anchors on tempo change so the position stays continuous. |
| `schedule` | `schedule_span(tracks, epoch, sample_rate, frames, &mut next_id)` — the **pure** look-ahead core: query the window, emit a `VoiceEvent` per onset (carrying the `delay`/`feedback`/`delay_mix` controls). `delay_config_for(ev, epoch, sr)` derives the `SetTrackDelay` command (cycle-fraction → frames) for the per-track delay bus. Shared by live + offline. |
| `transport` | `Transport<S: AudioSink>` — the real-time driver. Periodic `tick()` schedules `[now, now+lookahead]` in whole-cycle segments, emitting a `SetTrackDelay` before a voice when its track's delay-bus config changes; `set_cps` / `set_tracks` / `set_tempo_map` stage changes applied **quantized** at the next cycle boundary (no glitch, no clock reset). A `TempoMap` (`set_tempo_map`) drives a piecewise-constant tempo automation: the transport re-anchors the `Epoch` to the map's `cps` at each cycle boundary (a no-op mid-segment), so a scripted `tempo(...)` plays itself. |
| `render` | `render_offline(tracks, cps, cycles, &RenderConfig, path)` — non-real-time driver reusing the same scheduling + `Renderer`, writing WAV (24-bit/48k default) or Ogg via `hound`/`vorbis_rs`. Pre-scans the arrangement and preloads every `sample`/`audio` file source (best-effort) so file voices decode instead of falling back to synth, and configures each track's delay bus as it changes. Length is an explicit `cycles` count + tail. Fine under Arbor's job system. `render_offline_with_progress(…, start_cycle, cycles, …, on_progress, should_cancel)` is the same driver with: a **region window** (`start_cycle` — bounce `[start_cycle, start_cycle+cycles)`, onsets re-based onto the output's local timeline); a `FnMut(RenderProgress)` callback fired per block and a `Fn() -> bool` polled before each block for a live percentage + Stop (`RenderOutcome::Cancelled`, partial file finalized); and optional **LUFS normalization** (`RenderConfig::normalize` — meters the bounce with `ebur128` (ITU-R BS.1770) and applies one peak-limited gain to a target loudness). |
| `midi` | `export_midi(tracks, cps, cycles, path) -> MidiExportSummary` — the note-only dual of the bounce: walks the arrangement and writes a Standard MIDI File (`midly`), one MIDI track per nemus track (pitched haps → channel 0; recognised drum sounds → General-MIDI percussion on channel 9). Tempo from `cps` (one cycle = one 4/4 bar). |

## Why it's testable headless

The engine never names a device. It talks to an `AudioSink` (from the audio crate); the production
impl is a live ring-buffer producer, and `RecordingSink` is a recorder over a manual clock. So the
scheduler is exercised by advancing the fake clock and asserting on the recorded `VoiceEvent`s —
which is how Stage B is built before the real audio path exists.

## Scheduling policies

- **Query window.** `schedule_span` takes a half-open **frame** range, widens it to whole cycles
  (plus a guard cycle each side) for the pattern query, then re-filters emitted events by
  `start_frame ∈ [start, end)`. Adjacent windows are seamless: a hap on a seam is emitted by exactly
  one window.
- **Sustained dedup.** A sustained file source (`audio(...)`) is `pure` → one hap per cycle.
  `schedule_span` collapses the repeats within a call; across calls the `Transport` (and the offline
  render) keep a started-set keyed by `(track, path)`, cleared on track swap and `seek`.
- **Quantized swaps.** `set_cps` / `set_tracks` stage a change applied at the first cycle boundary
  at/after `scheduled_through`. A tempo change re-anchors the `Epoch` (frame stays continuous); a
  track change re-sends `ConfigureTracks` and resets the sustained dedup.
- **Back-pressure.** If the sink queue fills mid-tick, `tick` stops pushing and leaves
  `scheduled_through` unmoved, so the same near-future window is retried next tick — never blocking.

## Status

**Stage B + Onda 2 plumbing.** `clock::Epoch`, `schedule_span` / `voice_event_from_hap` (now carrying
the delay controls) / `delay_config_for`, `Transport::tick` (look-ahead refill + quantized swaps +
back-pressure + per-track delay reconfigure), and `render_offline` (file-source preload → block loop →
`Renderer::process` → WAV via `hound`, threading the delay-bus config) are written, with unit tests
over `RecordingSink` plus end-to-end offline-render tests in `tests/render_offline.rs`. The audio
`Renderer` (Onda 2 mixer/EQ/comp/reverb/delay) is the consumer of every additive seam delta.

Part of the nemus crate stack: `arbor-nemus-pattern` → `arbor-nemus-audio` → `arbor-nemus-engine`.
See [`design/nemus/architecture.md`](../../../design/nemus/architecture.md).
