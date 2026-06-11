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
| `VoiceParams` | per-voice DSP/mix, already sampled from the `ControlMap`: gain/pan/room/lpf/hpf/shift/speed/crush/shape/vel + the delay send (`delay`/`feedback`/`delay_mix`) |
| `AudioCommand` | `Voice` + transport/mixer controls: `ConfigureTracks`, `SetTrackGain`, `SetTrackPan`, `SetTrackMute`, `SetTrackSolo`, `SetMasterGain`, `SetTrackEq`, `SetMasterEq`, `SetTrackComp`, `SetMasterComp`, `SetTrackDelay`, `SetReverbIr`, `StopAll` |
| `EqBand` / `CompSettings` / `DelayConfig` / `ReverbIr` | strip-processor payloads carried on the mixer commands (never `ControlMap` fields, never language-visible) |
| `AudioSink` | the engine's view of the backend: `send(cmd)` (non-blocking) + `now_frame()` + `sample_rate()` |

### Mixer / strip processors (additive, Onda 2)

The seam's **frozen core** (absolute-frame timing, the `AudioSink` trait, `Renderer::process`,
`VoiceEvent`/`Frame`/`Epoch`) is unchanged. Onda 2 extends it **additively**:

- **Mixer**: per-track `gain` / `pan` / `mute` / `solo` (any soloed strip mutes the non-soloed
  ones) + a **master strip** (`SetMasterGain` + master EQ/comp), all driven by mixer commands.
- **Parametric EQ** (`SetTrackEq` / `SetMasterEq`): N bands, each `EqBand { kind, freq, gain_db, q }`
  where `kind ∈ peak | low-shelf | high-shelf | hpf | lpf` (RBJ cookbook biquads). Empty list = bypass.
- **Compressor** (`SetTrackComp` / `SetMasterComp`): standard feed-forward `CompSettings { threshold,
  ratio, attack, release, makeup, knee }` with a soft knee; `None` = bypass.
- **Convolution reverb** (`SetReverbIr`): the `room` send target is now a time-domain convolution over
  an impulse response — `ReverbIr::Procedural { seconds }` synthesises a deterministic IR (early
  reflections + decaying decorrelated tail), `ReverbIr::Buffer(Vec<Frame>)` installs an explicit IR
  (e.g. from a VSCO pack, downloaded in Onda 3). The renderer boots with a default procedural IR.
- **Delay bus** (`SetTrackDelay` + the `delay`/`feedback`/`delay_mix` `VoiceParams`): a per-track
  feedback echo. `delay_mix` is the per-event send into the bus; `delay` (line time) + `feedback`
  configure the bus (the engine converts `delay`'s cycle-fractions → frames). Echoes ring on
  independently of the source voice — distinct from `off` (which retriggers the pattern, in lang).

The engine owns cycle-time and does the cycle→frame mapping; audio owns the **sample clock** and
reports "now" back through `AudioSink::now_frame`. Resolution of a `Named` source (synth preset vs.
SFZ region vs. fallback) is the registry's job — the engine only forwards symbolic names.

**The implementation** (Stage A, this crate's body):

- `renderer.rs` — `Renderer`, the transport-agnostic DSP core. One `process(commands, out)` path
  drives **both** the real-time cpal callback and the offline render: it drains commands, starts
  voices sample-accurately within the block, then per strip applies EQ → comp → delay-bus mix-back →
  pan/gain with mute/solo, sums to the master strip (EQ → comp → gain), folds in the convolution
  reverb send, and runs the master limiter.
- `stream.rs` — `StreamSink` (the production `AudioSink`: an `rtrb` SPSC producer + a shared playhead
  atomic + a shared telemetry tap) and `open_output_stream(tracks, registry)` (cpal device/stream + the
  draining callback; the `registry` is baked into the `Renderer` here because it then lives inside the
  real-time callback, unreachable for a later swap). The callback never allocates, locks, or does IO.
- `meters.rs` — the **out-of-band telemetry tap** (`MeterTap` / `MeterSnapshot`), read via
  `StreamSink::peak()` (master only) or `StreamSink::meters()` (full). The callback writes, each device
  buffer, the master + **per-track** post-fader peak (decayed for smooth ballistics), the active
  **voice count**, and the **DSP load** (callback compute / buffer budget, EMA-smoothed). All lock-free
  atomics, like the playhead — additive, never part of the frozen command seam, never fed back into
  rendering. Per-track peaks are capped at `MAX_METER_TRACKS`.
- `registry.rs` — the **TOML sound registry**: short names (`bd`) and dotted names
  (`strings.violin`, `synth.pad`) → a synth preset / one-shot sample / SFZ instrument; unresolved →
  the default synth. Resolves `art` (articulations) + round-robin per onset. Manifest + articulation
  schema in the module docs (see "Registry articulations + round-robin" below). `instruments_list()`
  enumerates every resolvable instrument (name + kind) for the sound-bank UI.
- `voice.rs` — the per-voice DSP chain (source → hpf → lpf → shape → crush → gain×vel → pan → dry +
  room send + per-track delay send) and the fixed-capacity **voice pool** with the design's
  voice-stealing policy (quietest-releasing first, else oldest).
- `synth.rs` — the **default synth** (saw/square/sine/triangle oscillator + ADSR), grove's fallback
  and "electronic default" sound.
- `sampler.rs` + `sfz.rs` + `decode.rs` — the hand-written **SFZ subset parser** (VSCO 2 CE opcodes,
  plus `seq_length`/`seq_position` round-robin and `sw_last` keyswitches), the `SampleBank` of
  resident `Arc<[f32]>` audio, the RT-safe `SamplePlayer` (variable-rate linear-interpolation
  resampling for `shift`/`speed`/pitch), and the non-RT decoders (WAV via `hound`, mp3/ogg/flac via
  `symphonia`).
- `effects.rs` — hand-rolled `Biquad` (`lpf`/`hpf` + parametric-EQ bands), `shape` waveshaper,
  `crush` bitcrusher, the parametric `Eq` chain, a feed-forward `Compressor`, the `ConvReverb`
  convolution send bus, the per-track `DelayLine`, and a smoothed master `Limiter`.

### Registry articulations + round-robin

An `kind = "sfz"` registry entry may declare named **articulations** and the sampler does
**round-robin** sample selection:

```toml
[strings.violin]
kind = "sfz"
file = "vsco2/strings/violin.sfz"
# Articulation → keyswitch (a MIDI key activating regions tagged `sw_last=<midi>`):
art.legato.keyswitch    = 24
art.staccato.keyswitch  = 26
# …or → an alternate sample set in its own .sfz (loaded wholesale):
art.pizzicato.region    = "vsco2/strings/violin_pizz.sfz"
```

`.art("legato")` on a note resolves through the registry to the keyswitch (filtering the instrument's
regions) or the alternate region SFZ. **Round-robin**: an SFZ region group with `seq_length = N` and
`seq_position = 1..N` cycles its N variants per onset; the variant is chosen by a **deterministic
onset seed** (hashed from the engine's stable per-onset voice id), so playback is reproducible
loop-to-loop. A sparse group falls back to the first matching region.

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
command ring) · `fundsp` (declared; the EQ/comp/reverb/delay DSP is hand-rolled biquad/convolution to
keep the chain allocation-free and review-stable) · `symphonia` (mp3/ogg/flac import). Plus
`arbor-grove-pattern` for `ControlMap` / `SourceKind`.

## Status

**Stage A + Onda 2 (DSP + mixer).** The `seam` **frozen core** is intact (absolute-frame timing,
`AudioSink`, `Renderer::process`, `VoiceEvent`/`Frame`/`Epoch`); Onda 2 extends it **additively** with
the mixer/master commands, strip EQ/compressor, convolution reverb, per-track delay bus, and the
`delay`/`feedback`/`delay_mix` `VoiceParams`. `Renderer`, the cpal `stream`, the SFZ sampler (now with
articulations + round-robin) + default synth + TOML registry + effects + voice pool are implemented.
Compiled and validated by hand (the workspace does not auto-build).

Part of the grove crate stack: `arbor-grove-pattern` → `arbor-grove-audio` → `arbor-grove-engine`.
See [`design/grove/architecture.md`](../../../design/grove/architecture.md).
