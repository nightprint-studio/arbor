# merula-import

The **deterministic** MIDI → `.merula` converter for [merula](../../../design/merula), Arbor's
live-coding music editor — the "faithful" import path, with **no AI** anywhere. It begins once a MIDI
byte stream exists (off disk, or produced in memory by `merula-transcribe`) and ends with
idiomatic `.merula` source. It depends on `merula-pattern` (time, scales) and
`merula-lang` (AST + canonical emitter), and **not** on audio.

```text
MIDI bytes ──L1──▶ Song (notes in cycle-time) ──L2──▶ idiomatic .merula
                                              quantise · key · chords · loops
```

## Layers (each unit-tested in isolation)

| Module | Role |
|---|---|
| `transcode` | **L1** — pair note-on/off, convert ticks → cycles (exact rationals, no drift), read tempo → `cps`, split each MIDI track into a pitched part and a drum part (GM channel 10). Purely mechanical. |
| `quantize` | **L2** — snap onsets/durations to a `1/grid` grid; merge unisons that snap together. |
| `key` | **L2** — weighted pitch-class histogram → best-fit scale (incl. *hirajoshi*, *in-sen*, *iwato*, *kumoi*), plus `degree_of` (pitch → degree), the inverse of the pattern crate's `Scale::degree_to_midi`. |
| `chords` | **L2** — a simultaneous-pitch set → a chord symbol, matching against `merula-lang`'s own chord table (so emitted `'name`s are exactly what the evaluator accepts). Triads + sevenths; extensions fall back to `&` lanes. |
| `gm_drum` | General MIDI percussion key → merula sound name (`36 → bd`, …). |
| `emit` | Assemble the output as an `merula-lang` AST and print it through that crate's canonical emitter — so formatting lives in one place. Decides *structure*: degrees + `.scale(...)`, chord folding, `&` lanes for overlaps, `@`-weights for held notes, `<...>` collapse of repeating cycles, and **phrase factoring** — the timeline is split into short phrases, repeats are deduplicated into `let` bindings, and the track plays them through `arrange(section(...))` so a long take is editable phrases (chorus written once), not one inline pattern. |
| `build` | Tiny constructors for the lang AST nodes the emitter assembles (kept apart so `emit` stays about structure policy). |

The one-call entry is `convert::midi_to_merula(bytes, &ImportOptions)` (also `smf_to_merula`,
`midi_to_song`).

## Design notes

- **Interchange is `midly::Smf`.** The transcriber (WAV → MIDI) and this crate (MIDI → `.merula`)
  share no custom model — MIDI is the seam, kept in memory for the transient WAV → `.merula` path.
- **Degrees assume `ref_octave == default_octave`** (merula default `4`, C4 = 60). A track emits scale
  degrees only when *every* one of its notes fits the detected scale; otherwise absolute note names.
  A track with recognised chords uses absolute names + symbols (degrees and chords don't mix).
- **Graceful, not lossy-silent.** Unknown drum keys map to `perc`; notes spilling past a bar are
  clipped to the bar (v1); SMPTE-timecode MIDI is rejected with a clear error rather than guessed.

Part of the merula crate stack: `merula-pattern` + `merula-lang` → `merula-import`.
See [`design/merula/architecture.md`](../../../design/merula/architecture.md).
