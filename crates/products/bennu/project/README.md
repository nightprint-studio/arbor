# bennu-project

The Bennu **project / workspace model** — a **leaf crate** (docs §10). The analyzer
crates depend on *this*, never the reverse: capability detection gates which index
sources even get built, so it sits at the bottom of the graph.

Depends only on the shared contract (`bennu-proto`) + serde.

## What it owns

- **Capability detection** (`capability.rs`) — the Spike D ruleset. Tier A = pom
  dependency coordinate, tier B = config-file presence, tier C = source pattern. A
  capability activates on ≥1 strong (A/B) signal; a C-only match activates
  *provisionally* (recorded, low-confidence), never a hard-fail. The `hits` carry the
  evidence so the FE can explain the classification.
- **Maven pom parsing** (`pom.rs`) — lightweight targeted extraction (name, modules,
  dependency coordinates, `<properties>`, compiler source/target, toolchains). No XML
  crate (hard rule 7 — none on the approved list); a full config-graph XML model is
  `bennu-web`'s later job.
- **Encoding detection** (`encoding.rs`) — `project.build.sourceEncoding` → default
  UTF-8, plus per-path override. Decoding runs through `encoding_rs` (the WHATWG set), so
  UTF-8 / Cp1252 / ISO-8859-1 / … all decode natively; an unknown label degrades to lossy
  UTF-8 with the true label preserved. `decode_for_index` tries the declared encoding first
  and, when the bytes don't fit, recovers + flags the file `non_compliant` (for the index's
  encoding report). The round-trip save `encode` stays hand-rolled (Cp1252 / UTF-8) — its
  unmappable-char fallback semantics differ from `encoding_rs`' encoder.
- **Per-project JDK detection** (`jdk.rs`) — `maven.compiler.source/target`, compiler
  plugin, `<toolchains>`, plus override.
- **Project file tree** (`tree.rs`) — depth-bounded, dirs-first, noise-dirs skipped.
- **Open orchestration** (`model.rs`) — `open_project` → `ProjectInfo`, `read_file` →
  decoded `FileContents`.

## Tests

`#[cfg(test)]` proves it classifies a Struts/Entando pom vs a MyBatis pom correctly
(from dependency coordinates alone), plus pom / encoding / JDK round-trips.

## Usage

```rust
use bennu_project::prelude::*;
```
