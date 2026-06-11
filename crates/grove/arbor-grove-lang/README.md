# arbor-grove-lang

The `.grove` **language** layer of [grove](../../../design/grove), Arbor's live-coding music engine.
It turns source text into the pure [`Pattern`](../arbor-grove-pattern) algebra of **Fase 0** and back.
This is **Fase 1**: no audio, no scheduler — pure and unit-testable.

```text
source (.grove)  ──parse──▶  CST  ──walk──▶  AST  ──eval──▶  Pattern
                                              AST  ──emit──▶  source
```

## Layers (decoupled by design)

The layers never reach across each other (`design/grove/editing-model.md`):

- **Front end** — a [Tree-sitter](https://tree-sitter.github.io) grammar (`grammar.js`) plus a
  hand-written **external scanner** (`scanner.c`) for the context-sensitive bits the context-free
  grammar can't express: the island-mode switch on `s(`/`n(`, `$ident` holes, the `..`/`.`/float
  maximal munch, and `[]`/`<>` balancing (`design/grove/grammar.md §5`). Native byte ranges give every
  node a source span for free.
- **AST** (`ast.rs`) — the typed tree, the contract every other layer targets. Mirrors
  `design/grove/grammar.md` one-to-one; every node carries a `SourceSpan`.
- **Evaluator** — `AST → Pattern<ControlMap>`. Resolves the host language (let/fn/lambda/range/map,
  arithmetic, the closed stdlib of combinators + transforms) and the mini-notation islands.
- **Emitter** (`emit.rs`) — `AST → source` (pretty-printer). Deterministic, minimal-paren, semantic
  round-trip (comments/whitespace are not in the AST, so they are not recovered). The enabler for the
  future editor's surgical edits and for *materialisation*.
- **Materialiser** (`materialize.rs`) — evaluated haps → mini-notation AST: the value→AST half of
  *materialisation* (evaluate a generative sub-tree, re-emit it as a literal). Base scope: one cycle,
  discrete events, overlap split into `&` lanes, uniform grid with `~`/`@`.

## Build workflow (Tree-sitter)

The generated parser is **C code committed to the repo** (`src/parser.c`), so a plain `cargo build`
needs no Node and no Tree-sitter CLI — only a C compiler, the same one the workspace already
requires (vendored libgit2 / mlua). The CLI is needed **only to regenerate** the parser after a
grammar change.

### Files

| File | Role | Hand-written? |
|---|---|---|
| `grammar.js` | the Tree-sitter grammar (Standard level) | ✏️ yes |
| `src/scanner.c` | external scanner (island-mode switch, leaves, numeric munch) | ✏️ yes |
| `build.rs` | compiles `parser.c` + `scanner.c` with the `cc` crate | ✏️ yes |
| `package.json` | `{ "type": "commonjs" }` — see below | ✏️ yes |
| `src/parser.c`, `src/grammar.json`, `src/node-types.json`, `src/tree_sitter/*.h` | generated | ⚙️ committed, do not edit |

**Why `package.json`?** The repo root's `package.json` declares `"type": "module"` (for the ESM
frontend), which makes Node treat *every* `.js` — including `grammar.js` — as an ES module, so its
`module.exports` (CommonJS, which Tree-sitter requires) throws *"module is not defined in ES module
scope"*. Node resolves the nearest `package.json` walking up the tree, so this local one with
`"type": "commonjs"` shadows the root **only for this crate's subtree**, leaving the frontend
untouched.

### Install the CLI (once)

A global install — the directory you run it from doesn't matter. Pin it to the runtime's major so
the generated ABI matches:

```sh
cargo install --locked tree-sitter-cli --version "^0.25"
```

### Regenerate the parser

Run from the crate root (where `grammar.js` lives):

```sh
cd crates/grove/arbor-grove-lang
tree-sitter generate     # writes src/parser.c + src/grammar.json + src/node-types.json + src/tree_sitter/*.h
```

`generate` is pure codegen (no C compiler needed). It prints any unresolved grammar conflicts — fix
them in `grammar.js` (`prec` / `conflicts`) and re-run. To eyeball the tree on a snippet:
`tree-sitter parse some.grove` (this one *does* compile the parser, so it needs the C toolchain).

> The *"No `tree-sitter.json` … using ABI version 14"* warning is harmless — the `tree-sitter 0.25`
> runtime reads ABI 14 fine. Add a `tree-sitter.json` only if you want ABI 15 / to silence it.

### Edit cycle

- Changed **`grammar.js`** → re-run `tree-sitter generate`, then `cargo build`, then commit the
  regenerated `src/parser.c` & friends alongside it.
- Changed **only `src/scanner.c`** → just `cargo build`; `generate` is not involved (`build.rs` has
  `rerun-if-changed` on the scanner). No commit of generated files needed.

`s` / `sound` / `n` / `note` are **reserved island keywords** (recognised by the scanner, not
grammar literals), and a host identifier shaped exactly like a pitch (`c4`, `ef3`) lexes as a note
literal — both are deliberate, Rust-style reservations. No WASM build is involved here; that belongs
to the future CodeMirror editor (a separate `tree-sitter build --wasm` step), not this crate.

## Consumes from `arbor-grove-pattern`

The evaluator builds patterns out of the Fase 0 stdlib. A few mini-notation operators map onto
primitives added there for this layer:

- `@n` / `_` (weight / elongate) → `timecat` (weighted slots).
- `(n,k[,rot])` (Euclidean) → `Pattern::euclid` (Bjorklund).
- leaf source spans → `Pattern::tag_span`.

`!n` (replicate) and `'chord` (chord expansion) stay AST-level expansions; the chord interval table
lives here (the pattern crate owns only scales).

## Status

The text↔Pattern loop is complete. Dependencies: the pattern crate, plus `tree-sitter` (runtime)
and `cc` (build-dep) for the front end.

- the **AST** (`ast.rs`) and **span-aware errors** (`error.rs`) — the pure contract;
- the **front end** (`parse.rs`): the Tree-sitter `grammar.js` + external `scanner.c`, the committed
  generated `parser.c` compiled by `build.rs`, and the CST→AST walker — `source → Program`, with
  syntax errors located by span. Host **pitch literals** (`c4`, octave mandatory) are supported so
  `choose(c4, ef4, g4)` works. Exercised by `tests/roundtrip.rs`.
- the **evaluator** (`AST → Pattern`): host language (let/fn/lambda, ranges + `.map`/`.par`/`.seq`/
  `.cat`, arithmetic), the transform-value model and closed stdlib (`combinators` + `transforms`),
  mini-notation islands (`island`), totality (`totality` + a runtime depth guard), and injected
  capabilities (`SourceLoader` for `import`, `LogSink` for logging). Exercised by `tests/eval.rs`.
- the **emitter** (`AST → source`) and **materialiser** (haps → mini-notation AST). Exercised by
  `tests/emit.rs` and `tests/materialize.rs`. The emitter is the inverse the parser reads back:
  `parse(emit(ast))` ≈ `ast` modulo spans (a *semantic* round-trip — comments and incidental
  whitespace are not in the AST; `sound`/`note` aliases re-emit as `s`/`n`).

Reach the API through the prelude (workspace convention): `use arbor_grove_lang::prelude::*;`.

Part of the grove crate stack: `arbor-grove-pattern` → **`arbor-grove-lang`** / `arbor-grove-audio` →
`arbor-grove-engine`. See [`design/grove/architecture.md`](../../../design/grove/architecture.md).
