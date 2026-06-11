# arbor-grove-engine

The **timing runtime** of [grove](../../../design/grove), Arbor's live-coding music engine. This is
**Fase 3**: it queries the pure patterns and fires sample-accurate triggers at the audio backend, in
time. It depends on `arbor-grove-pattern` + `arbor-grove-audio` — and deliberately **not** on
`arbor-grove-lang`: the shell evaluates `.grove` source → `Tracks<ControlMap>` and hands those in;
the engine speaks only patterns + the audio seam.

```text
Tracks<ControlMap> ──query look-ahead──▶ VoiceEvents ──AudioSink──▶ audio
```

## Pieces

| Module | Role |
|---|---|
| `clock` | `Epoch` — maps cycle-time ↔ output frames (`frames_per_cycle = sample_rate / cps`). Audio owns the sample clock; the engine owns `cps` and re-anchors on tempo change so the position stays continuous. |
| `schedule` | `schedule_span(tracks, epoch, sample_rate, frames, &mut next_id)` — the **pure** look-ahead core: query the window, emit a `VoiceEvent` per onset. Shared by live + offline. |
| `transport` | `Transport<S: AudioSink>` — the real-time driver. Periodic `tick()` schedules `[now, now+lookahead]`; `set_cps` / `set_tracks` stage changes applied **quantized** at the next cycle boundary (no glitch, no clock reset). |
| `render` | `render_offline(tracks, cps, &RenderConfig, path)` — non-real-time driver reusing the same scheduling + `Renderer`, writing WAV (24-bit/48k default) via `hound`. Fine under Arbor's job system. |

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

**Stage B implemented.** `clock::Epoch`, `schedule_span` / `voice_event_from_hap`, `Transport::tick`
(look-ahead refill + quantized swaps + back-pressure), and `render_offline` (block loop →
`Renderer::process` → WAV via `hound`) are written, with unit tests over `RecordingSink`. The audio
`Renderer::process` is still `unimplemented!` (Stage A, parallel), so the offline render path compiles
but cannot produce audio until that lands.

Part of the grove crate stack: `arbor-grove-pattern` → `arbor-grove-audio` → `arbor-grove-engine`.
See [`design/grove/architecture.md`](../../../design/grove/architecture.md).
