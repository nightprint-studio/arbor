# arbor-grove-audio

The real-time / DSP layer of [grove](../../../design/grove), Arbor's live-coding music engine — the
only grove crate that touches the audio hardware. This is **Fase 2**: it turns the engine's
sample-accurate trigger events into sound, and doubles as the DSP core for the offline render.

```text
VoiceEvent (from the engine) ──▶ Renderer (voices → mixer → effects → master) ──▶ frames
```

## Two halves

**The contract** (`seam.rs`) — frozen so `arbor-grove-engine` is developed against it in parallel:

| Type | Role |
|---|---|
| `VoiceEvent { id, start_frame, dur_frames, source, note, params, track, span }` | one sample-accurate trigger; times are **absolute output frames** |
| `VoiceSource` | `Named { sound, variant, inst, art }` (resolved by the registry) or `File { path, kind }` (one-shot / sustained) |
| `VoiceParams` | per-voice DSP/mix, already sampled from the `ControlMap` (gain/pan/room/lpf/hpf/shift/speed/crush/shape/vel) |
| `AudioCommand` | `Voice` + transport/mixer controls (`ConfigureTracks`, `SetTrackGain`, `SetTrackMute`, `StopAll`) |
| `AudioSink` | the engine's view of the backend: `send(cmd)` (non-blocking) + `now_frame()` + `sample_rate()` |

The engine owns cycle-time and does the cycle→frame mapping; audio owns the **sample clock** and
reports "now" back through `AudioSink::now_frame`. Resolution of a `Named` source (synth preset vs.
SFZ region vs. fallback) is the registry's job — the engine only forwards symbolic names.

**The implementation** (Stage A, this crate's body):

- `renderer.rs` — `Renderer`, the transport-agnostic DSP core. One `process(commands, out)` path
  drives **both** the real-time cpal callback and the offline render: it drains commands, starts
  voices sample-accurately within the block, mixes per-track strips, sends to the reverb bus, and
  runs the master limiter.
- `stream.rs` — `StreamSink` (the production `AudioSink`: an `rtrb` SPSC producer + a shared playhead
  atomic) and `open_output_stream` (cpal device/stream + the draining callback). The callback never
  allocates, locks, or does IO.
- `registry.rs` — the **TOML sound registry**: short names (`bd`) and dotted names
  (`strings.violin`, `synth.pad`) → a synth preset / one-shot sample / SFZ instrument; unresolved →
  the default synth. Manifest schema is in the module docs.
- `voice.rs` — the per-voice DSP chain (source → hpf → lpf → shape → crush → gain×vel → pan → dry +
  room send) and the fixed-capacity **voice pool** with the design's voice-stealing policy
  (quietest-releasing first, else oldest).
- `synth.rs` — the **default synth** (saw/square/sine/triangle oscillator + ADSR), grove's fallback
  and "electronic default" sound.
- `sampler.rs` + `sfz.rs` + `decode.rs` — the hand-written **SFZ subset parser** (VSCO 2 CE opcodes),
  the `SampleBank` of resident `Arc<[f32]>` audio, the RT-safe `SamplePlayer` (variable-rate
  linear-interpolation resampling for `shift`/`speed`/pitch), and the non-RT decoders
  (WAV via `hound`, mp3/ogg/flac via `symphonia`).
- `effects.rs` — hand-rolled `Biquad` (`lpf`/`hpf`), `shape` waveshaper, `crush` bitcrusher, a
  Schroeder/Freeverb-style stereo reverb send bus, and a smoothed master limiter.

`RecordingSink` (`testing.rs`) is a non-real-time `AudioSink` that records commands against a
manual clock — the seam that lets the engine's scheduler be tested headless.

### Sample-bank / RT discipline

Decoding and SFZ/sample loading happen on a **non-RT** path (`Renderer::registry_mut` /
`Renderer::preload_file`) and the audio is kept resident as `Arc<[f32]>`. The cpal callback only ever
*reads* resident data; a `Named` name or a `File` path that isn't resident falls back to the synth
rather than blocking. `rubato` is reserved for the offline fixed-ratio sample-rate conversion path;
per-voice pitch in the RT path is linear-interpolation resampling through the resident buffer.

## Dependencies

`cpal` (stream) · `hound` (WAV) · `rubato` (resampling for `shift`/`speed`) · `rtrb` (lock-free SPSC
command ring) · `fundsp` (reverb/filters) · `symphonia` (mp3/ogg/flac import). Plus
`arbor-grove-pattern` for `ControlMap` / `SourceKind`.

## Status

**Stage A (DSP implemented).** The `seam` contract is frozen; `Renderer`, the cpal `stream`, the SFZ
sampler + default synth + TOML registry + effects + voice pool are implemented. `delay` is **not**
here — it isn't a `ControlMap` field yet (Fase 5). Compiled and validated by hand (the workspace
does not auto-build).

Part of the grove crate stack: `arbor-grove-pattern` → `arbor-grove-audio` → `arbor-grove-engine`.
See [`design/grove/architecture.md`](../../../design/grove/architecture.md).
