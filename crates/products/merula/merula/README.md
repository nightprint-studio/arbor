# merula

The **facade** of [merula](../../../design/merula), Arbor's live-coding music engine. merula is four
library crates with one-way dependencies; this crate re-exports their public surfaces through **one**
curated prelude so the Arbor shell writes a single `use merula::prelude::*;`.

```text
        merula-pattern        pure pattern algebra (zero external deps)
         ↑               ↑
merula-lang   merula-engine ← merula-audio
         ↑               ↑                    ↑
         └──────── merula (this crate) ──┘
                          ↑
                 src-tauri (shell — not a merula crate)
```

## What it does

Almost nothing — by design. It depends on the four crates and stitches their preludes together.
The four preludes nearly glob-merge cleanly: most of the surface is single-homed, and the shared
types (`ControlMap`, `Pattern`, `SourceKind`, `TimeSpan`, …) are *identical* re-exports that Rust
deduplicates. The **only** genuine collision is the per-crate `Result` alias (four distinct
`Result<T, …Error>`).

So the facade **curates** the prelude: it re-exports every public item by name **except** the four
`Result` aliases, and provides a single unified [`MerulaError`] + `Result` in their place. The four
underlying error types stay reachable by their own names (`PatternError`, `LangError`, `AudioError`,
`EngineError`) and convert into `MerulaError` with `?` — which is exactly what the shell wants when it
maps a merula failure to `AppError` at the IPC boundary.

There are deliberately **no** per-crate namespaced modules (`merula::lang::…`); the curated prelude is
the one canonical surface.

## Usage

```rust
use merula::prelude::*;

fn run(source: &str) -> Result<()> {           // facade `Result` = Result<_, MerulaError>
    let out = evaluate(source, &EvalConfig::default(), &NoImports, &SilentLog)?; // LangError → MerulaError
    render_offline(&out.tracks, out.cps, 8, &RenderConfig::default(), "out.wav".as_ref())?; // EngineError → MerulaError
    Ok(())
}
```

## Maintenance

When a merula crate adds a public item to *its* prelude, add it to this facade's `prelude.rs`
same-turn (the workspace prelude discipline, one level up). The curated list is the single place that
drifts; keeping it in sync is the whole cost of the facade.

Part of the merula crate stack. See [`design/merula/architecture.md`](../../../design/merula/architecture.md).
