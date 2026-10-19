# tree-sitter-java — Arbor's vendored copy

Upstream: <https://github.com/tree-sitter/tree-sitter-java> at `e10607b` (2025-09-15), identical to
the crates.io release 0.23.5. The workspace `[patch.crates-io]` points here; nothing else changes.

Only the Rust binding is kept (`bindings/rust`, `src/`, `grammar.js`, `queries/`, `test/`). The
`dev-dependencies` were dropped so the vendored crate pulls no second `tree-sitter`.

## What differs from upstream

Every edit in `grammar.js` is marked `arbor:`.

| Change | Java | Node |
|---|---|---|
| a qualified name before a record pattern's components | 21 | `record_pattern` accepts `scoped_type_identifier` |
| `final` and `_` in a type pattern / record component | 21–22 | `type_pattern`, `record_pattern_component` |
| statements before `this(…)` / `super(…)` | 25 (JEP 513) | `constructor_body` |
| `import module java.base;` | 25 (JEP 511) | new `module_import_declaration` |
| `@interface A extends B` (not `implements`), and type parameters, parameters or `throws` on an element | — | javac parses them and rejects them *semantically*, so Arbor's checks need a tree, not a syntax error |

## Regenerating

```sh
cd third_party/tree-sitter-java
tree-sitter generate     # rewrites src/parser.c, src/node-types.json, src/grammar.json
tree-sitter test         # test/corpus/arbor.txt holds the cases above
```

`parser.c` is generated: edit `grammar.js`, never the C.

## Upgrading

Diff the new upstream `grammar.js` against this one, re-apply the `arbor:` hunks, regenerate, run
`tree-sitter test` and `cargo test -p bennu-intel --test check_corpus`.
