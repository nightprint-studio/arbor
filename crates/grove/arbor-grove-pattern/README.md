# arbor-grove-pattern

The pure, deterministic **pattern algebra** at the heart of [grove](../../../design/grove), Arbor's
live-coding music engine. This is **Fase 0**: the time/event model and the closed standard library of
combinators — no language, no audio, no scheduler.

```text
Pattern<T> = (TimeSpan) -> [Hap<T>]
```

You query a pattern over a window of cycle-time and it returns the events in that window. Everything
else (the `.grove` language, the synth/sampler, the real-time scheduler) is built on top in later
crates.

## Why it looks the way it does

- **Exact rational time** (`Time`, hand-rolled `i64` num/den + `gcd`, `i128` intermediates). Cycle
  subdivisions are fractions like `1/3`, `1/6`, `1/12`; `f64` would drift and haps would stop landing
  on cycle boundaries — breaking determinism and the offline render. Same choice as Tidal.
- **Absolute timeline.** Patterns are queried at the true cycle `N`, never auto-wrapped to cycle 0.
  Looping is a transform (`arrange`'s wrap), not a baked-in behaviour — that's what enables long-form
  arrangements without a refactor.
- **Source spans from day one.** Every `Hap` carries an optional `SourceSpan` (byte offsets) so the
  live editor can highlight exactly the characters that are sounding; the language layer stamps them
  with `Pattern::tag_span` (inner leaf spans win over outer containers).
- **Per-cycle seeded RNG.** `rand`, `choose`, `degrade`, `sometimes` derive randomness from the
  event's onset time — the same cycle always makes the same choices, so a loop is bit-identical and a
  re-eval after an edit never disturbs cycles already fixed.
- **Generic `Pattern<T>`.** Structural combinators are value-agnostic, so timing is testable with
  trivial payloads (`Pattern<i32>`, `Pattern<&str>`). The real grove value is `ControlMap`, a typed
  bag of controls; voice/mix transforms are specialised to `Pattern<ControlMap>`.
- **Zero external dependencies** (std only). Compiles and tests instantly; trivially splittable.

## The model in three types

| Type | What it is |
|---|---|
| `Time` | exact rational number of cycles |
| `TimeSpan { begin, end }` | half-open window `[begin, end)` — a query window, or a hap's `whole`/`part` |
| `Hap<T> { whole, part, value, span }` | one event: `whole` = full extent (`None` for continuous signals), `part` = the queried fragment, `value` = payload, `span` = source bytes |

A hap **has an onset** in a query when `part.begin == whole.begin`.

> Query results are **not** guaranteed to be in time order (Tidal-style). Consumers that need a
> sequence — the scheduler, or an assertion — sort by `part.begin`.

## Standard library (closed)

| Group | Items |
|---|---|
| Composition | `pure` · `silence` · `stack`/`par` · `fastcat`/`seq` · `timecat` · `slowcat`/`cat` · `arrange` · `cycles` · `tracks` · `track` |
| Time/structure | `fast` · `slow` · `rev` · `every` · `off` · `late` · `early` |
| Rhythm/probability | `degrade` · `degrade_by` · `sometimes` · `sometimes_by` · `euclid` |
| Voice/mix | `gain` · `pan` · `room` · `lpf` · `hpf` · `shift` · `speed` · `crush` · `shape` · `vel` · `inst` · `art` · `scale` · `jux` |
| Generative | `rand` · `choose` |
| File sources | `sample` · `audio` (markers only — decode/playback is the audio crate) |

Constructors and generators are free functions; transforms are methods on `Pattern`. The
transform-value / partial-application duality (`fast(2)` as a standalone value) is a language-layer
concern — here a "transform value" passed to `every`/`off`/`sometimes`/`jux` is a Rust closure.

## Usage

Reach the API through the prelude (workspace convention):

```rust
use arbor_grove_pattern::prelude::*;

// four-on-the-floor kick + offbeat hats
let beat = stack(vec![
    pure(ControlMap::sound("bd")).fast(Time::int(4)),
    pure(ControlMap::sound("hh")).fast(Time::int(8)),
]);
let haps = beat.query(TimeSpan::cycle(0));

// a melody of scale degrees, resolved to pitches
let melody = fastcat(vec![
    pure(ControlMap::degree(0)),
    pure(ControlMap::degree(2)),
    pure(ControlMap::degree(4)),
])
.scale(Scale::parse("c:minor").unwrap(), 4)
.gain(rand(0.4, 0.8)); // per-event random gain, deterministic per cycle
```

## Layout

```
src/
  time.rs        exact rational Time
  span.rs        TimeSpan + SourceSpan
  hap.rs         Hap<T>
  pattern.rs     Pattern<T> + internal query helpers (split_queries, with_*_time, fmap, filter_haps)
  rng.rs         deterministic time-seeded RNG
  control.rs     ControlMap (the concrete value)
  pitch.rs       note names, scales, degree resolution
  combinators/   the stdlib, grouped (compose / time / rhythm / voice / generative / source)
  error.rs       PatternError
  prelude.rs     canonical public surface
```

Part of the grove crate stack: `arbor-grove-pattern` → `arbor-grove-lang` / `arbor-grove-audio` →
`arbor-grove-engine`. See [`design/grove/architecture.md`](../../../design/grove/architecture.md).
