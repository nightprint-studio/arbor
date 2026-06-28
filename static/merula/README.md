# merula — Tree-sitter WebAssembly assets

The merula editor (CodeMirror 6) parses `.merula` source **in the WebView** with the
same `merula-lang` Tree-sitter grammar the backend uses, via
[`web-tree-sitter`](https://www.npmjs.com/package/web-tree-sitter). Two `.wasm`
files must live here (served at `/merula/*.wasm`):

| File | What it is | Source |
|---|---|---|
| `tree-sitter.wasm` | the web-tree-sitter Emscripten runtime core | copied from `node_modules/web-tree-sitter/tree-sitter.wasm` |
| `tree-sitter-merula.wasm` | the **merula grammar** compiled to wasm | built from the crate (below) |

Both are **committed** (like the generated `parser.c` in the crate) so the app
build stays hermetic — no Node / Emscripten step in `vite build`.

## Quick path (Windows)

From the repo root, run **`build-merula-wasm.bat`** — it builds the grammar wasm
and copies both files here. The manual steps below are what it automates.

## Building / refreshing the assets

> Needs the `tree-sitter` CLI and **Emscripten** (`emcc`) or Docker — the wasm
> compile is separate from the C toolchain used by the Rust crate. Re-run this
> whenever `grammar.js` / `scanner.c` change (after `tree-sitter generate`).

> The CLI (≥ 0.24) requires a `tree-sitter.json` in the grammar crate — it is
> committed there. Without it `build --wasm` fails with "Failed to locate a
> tree-sitter.json file".

```sh
# 1. grammar → wasm (run inside the grammar crate)
cd crates/merula/merula-lang
tree-sitter build --wasm            # emits tree-sitter-merula.wasm

# 2. place both wasm files here (paths relative to repo root)
cp crates/merula/merula-lang/tree-sitter-merula.wasm static/merula/
cp node_modules/web-tree-sitter/tree-sitter.wasm        static/merula/
```

The grammar wasm targets the ABI emitted by the committed `parser.c` (ABI 14),
which `web-tree-sitter` ^0.25 loads. If `Language.load` fails at runtime the
editor degrades gracefully to **plain text** (no highlight / lint) rather than
crashing — check that both files exist here and were rebuilt after a grammar
change.
