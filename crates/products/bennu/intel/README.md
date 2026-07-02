# bennu-intel

The Bennu **code-intel provider seam** — **Phase-0 skeleton**.

The heart of the abstraction (docs §2): the FE speaks **one** protocol for every
language via the `IntelProvider` trait (completion / hover / definition / references
/ diagnostics / rename / format / symbols). Java goes to the native, index-backed
engine; Rust *will* go to rust-analyzer via LSP — the "predisposed LSP" the design
requires.

Two impl slots:

- **`NativeJavaProvider`** — the MVP impl, index-backed. **Phase 1 implements
  member-access completion end to end**: it holds an `IndexResolver` over the built
  project index (`bennu-index`) + the JDK member index (`bennu-classpath`), infers the
  receiver type at the caret (`bennu-java`), walks its members (superclass + interfaces),
  and prefix-filters into `CompletionItem`s. Constructed empty via `new()` (before a
  project is open / while the index is still building), it answers completion with the
  benign empty list. Hover / definition / references / rename / format stay stubbed.
- **`LspClientProvider`** — the **predisposed** rust-analyzer slot. Documented and
  present, **not implemented in the MVP** (tower-lsp deferred — docs §4); its methods
  return `IntelError::Unimplemented`. Later LSP wiring is a fill-in of these bodies,
  not a new shape.

## Phase-1 completion machinery

- **`java_index`** — turns a project's `.java` sources into `bennu-index` `IndexRecord`s
  (each type a `Class` symbol whose `members_json` is its resolved member surface).
- **`resolver`** — `IndexResolver`, the `bennu-java` `TypeResolver` over the persisted
  project index + JDK member index. Converts `bennu-classpath`'s member shape into the
  `bennu-java` seam shape at the boundary (`convert_members`).
- **`jdk`** — `JdkMemberIndex`, a mutex-serialized `Send + Sync` wrapper around the boxed
  JDK classpath source (the JDK-8 `JarSource` is `!Sync`; the mutex restores `Sync` so the
  provider can live in the multi-threaded backend state).
- **`completion`** — the caret → candidates query.

## Usage

```rust
use bennu_intel::prelude::*;
```
