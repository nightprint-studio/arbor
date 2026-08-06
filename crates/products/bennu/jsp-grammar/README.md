# tree-sitter-jsp / `bennu-jsp-grammar`

The **JSP** tree-sitter grammar, with **two consumers and one generate**:

- the **frontend** loads `static/bennu/tree-sitter-jsp.wasm` through `web-tree-sitter`
  (`src/lib/components/bennu/jsp-lang.ts`) to colour a page;
- the **backend** links the same `src/parser.c` natively — this directory is also the
  `bennu-jsp-grammar` Cargo crate (`build.rs` compiles the C with `cc`, exactly as
  `picus-parse` and `merula-lang` do), exposing `prelude::{jsp_language, parse_jsp}`.

Both builds come from the same `tree-sitter generate`, which is what makes "the syntax-tree
panel shows what the highlighter saw" a fact rather than a hope. The Rust side arrived with
the two features that need a real tree rather than the tolerant tag scan: the syntax-tree /
model panel, and structural search — which compares nodes, so a pattern and a page have to be
read by one grammar or nothing matches.

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
- **EL / OGNL are decomposed, but not into a precedence tree.** `${…}` / `#{…}` / `%{…}` hold a
  flat run in which an `el_path` (a name and what is read off it — `.prop`, `[i]`, `(args)`) is
  a subtree and operators, literals and whitespace are its siblings. A full expression grammar
  buys nothing for the questions anyone asks of a page — which are all about paths — and costs
  conflicts, a whitespace rule per operand (`extras: []` is global) and a new way for a
  half-typed line to blow up. Every character between the braces is consumed by some token
  (`el_other` is the one-character last resort), so **a body cannot fail to parse**; the one
  thing it will not cross is a `<` that starts a tag, which is what keeps an unterminated `${`
  — the state of the file between two keystrokes — from swallowing the rest of the page.

Highlighting is leaf-driven by the `classify` function in `jsp-lang.ts` (node type →
`TokenClass`), not a `.scm` query — the same model as `java-lang.ts`.

## Build

`cargo build` needs **neither** of the tools below — it compiles the committed `src/parser.c`
with `cc`. They are only for regenerating the grammar after editing `grammar.js`, and the
wasm must be rebuilt in the same pass or the two consumers drift apart.

Requires the tree-sitter CLI (`cargo install --locked tree-sitter-cli --version "^0.25"`)
and Docker (for the wasm compile, since Emscripten isn't installed locally). ABI 14.

```sh
cd crates/products/bennu/jsp-grammar
tree-sitter generate                 # grammar.js → src/parser.c + metadata
tree-sitter build --wasm             # src/parser.c → tree-sitter-jsp.wasm (via Docker)
cp tree-sitter-jsp.wasm ../../../../static/bennu/tree-sitter-jsp.wasm
```

To eyeball the tree on a snippet: `tree-sitter parse some.jsp`.
