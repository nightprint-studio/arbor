# bennu-core

The **canonical headless state** the `bennu-be` process owns: [`BennuState`].

The bennu twin of `tyto-core` / `sitta-core` — **Tauri-free by construction**.
Deliberately small: a Java analyzer's heavy state (the mmap'd symbol index, the
classpath sources, per-project models) lives in the leaf analysis crates
(`bennu-index`, `bennu-classpath`, `bennu-project`, …) that the domain handlers own.
This state carries only:

- the **BE→FE event egress** (`emit` / `event_sink`), re-emitted by the shell to the
  Bennu window;
- the **reverse channel** back to the shell (`host_call` / `host_caller`), for host
  round-trips like reveal-in-explorer / open-path.

It also owns the typed **product config** (`config.toml`, per-profile) — editor
defaults plus the per-project JDK / encoding *overrides* the project model consults.
The path is resolved in-process (`arbor_core::prelude::bennu_config_path`), never
pushed by the shell.

## Usage

```rust
use bennu_core::prelude::*;
```
