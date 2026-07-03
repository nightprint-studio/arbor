# tree-sitter-jsp

The **JSP** tree-sitter grammar for Bennu's Java-editor highlighter. Unlike `merula-lang`,
this is **FE-only** — there is no Rust consumer, so no Cargo crate: just `grammar.js` + the
generated `src/`, compiled to `static/bennu/tree-sitter-jsp.wasm` and loaded by
`src/lib/components/bennu/jsp-lang.ts` via `web-tree-sitter`.

## Why a custom grammar

`@codemirror/lang-html` mis-tags namespaced taglib **closing** tags (`</s:iterator>`,
`</c:if>`, `</jsp:include>`) as *invalid* (dark red) or leaves them untagged (white), and
its embedded-JS parser breaks when JSP tags interleave. A native grammar parses the real
JSP shapes — namespaced tags, scriptlets `<% … %>` / directives `<%@ … %>` / declarations
/ expressions, JSP comments `<%-- … --%>`, EL `${…}` / `#{…}`, Struts OGNL `%{…}` (incl.
inside attribute values) — so every construct colours correctly.

## Design

- **Flat tag model** (no open/close nesting): `start_tag` / `end_tag` / `self_closing_tag`
  each expose their `tag_name`, so both open and close colour. Highlighting only needs
  leaf tokens, not a matched tree.
- **`<% … %>` family + comments are single leaf tokens**, disambiguated by *lexical
  precedence* (comment > directive/declaration/expression > scriptlet) since they share
  the `<%` prefix. Bodies match "a run of chars that isn't the terminator" — no lazy
  quantifiers, **no external C scanner**.
- **`extras: []`** — tag whitespace is explicit; `text` keeps its own. Forgiving: a stray
  `<` / unterminated block falls back to `text`, never an ERROR that poisons highlighting.

Highlighting is leaf-driven by the `classify` function in `jsp-lang.ts` (node type →
`TokenClass`), not a `.scm` query — the same model as `java-lang.ts`.

## Build

Requires the tree-sitter CLI (`cargo install --locked tree-sitter-cli --version "^0.25"`)
and Docker (for the wasm compile, since Emscripten isn't installed locally). ABI 14.

```sh
cd crates/products/bennu/jsp-grammar
tree-sitter generate                 # grammar.js → src/parser.c + metadata
tree-sitter build --wasm             # src/parser.c → tree-sitter-jsp.wasm (via Docker)
cp tree-sitter-jsp.wasm ../../../../static/bennu/tree-sitter-jsp.wasm
```

To eyeball the tree on a snippet: `tree-sitter parse some.jsp`.
