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
- **Emitter** — `AST → source` (pretty-printer). The enabler for the future editor's surgical edits
  and *materialisation* (evaluate a generative sub-tree, re-emit it as a literal).

## Build workflow (Tree-sitter)

The generated parser is **C code committed to the repo**, so a plain `cargo build` needs no Node:

1. Edit `grammar.js` / `scanner.c`.
2. Regenerate once with the Tree-sitter CLI (a **dev-time, manual** tool — never in the cargo build
   path): `tree-sitter generate`. This writes `src/parser.c` (+ `src/node-types.json`), which are
   committed.
3. `build.rs` compiles `src/parser.c` + `src/scanner.c` with the `cc` crate. The C toolchain is the
   same one already required by the workspace (vendored libgit2 / mlua), so there is nothing new to
   install for building — only the CLI for regenerating.

## Consumes from `arbor-grove-pattern`

The evaluator builds patterns out of the Fase 0 stdlib. A few mini-notation operators map onto
primitives added there for this layer:

- `@n` / `_` (weight / elongate) → `timecat` (weighted slots).
- `(n,k[,rot])` (Euclidean) → `Pattern::euclid` (Bjorklund).
- leaf source spans → `Pattern::tag_span`.

`!n` (replicate) and `'chord` (chord expansion) stay AST-level expansions; the chord interval table
lives here (the pattern crate owns only scales).

## Status

Staged build (still zero external dependencies — only the pattern crate).

Present:

- the **AST** (`ast.rs`) and **span-aware errors** (`error.rs`) — the pure contract;
- the **evaluator** (`AST → Pattern`): host language (let/fn/lambda, ranges + `.map`/`.par`/`.seq`/
  `.cat`, arithmetic), the transform-value model and closed stdlib (`combinators` + `transforms`),
  mini-notation islands (`island`), totality (`totality` + a runtime depth guard), and injected
  capabilities (`SourceLoader` for `import`, `LogSink` for logging). Exercised by `tests/eval.rs`,
  which builds ASTs by hand and asserts on the resulting haps.

Coming next, against this AST:

- the **emitter** (`AST → source`);
- the **Tree-sitter front end** — `grammar.js` + external `scanner.c` + `build.rs` + the CST→AST
  walker (filling in `parse.rs`), at which point `tree-sitter` (runtime) and `cc` (build-dep) join
  `Cargo.toml`. Until then `parse()` returns a "not wired" error, so `import` (which parses loaded
  modules) is plumbed but inert.

Reach the API through the prelude (workspace convention): `use arbor_grove_lang::prelude::*;`.

Part of the grove crate stack: `arbor-grove-pattern` → **`arbor-grove-lang`** / `arbor-grove-audio` →
`arbor-grove-engine`. See [`design/grove/architecture.md`](../../../design/grove/architecture.md).
