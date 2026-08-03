# bennu-ext

The seam that makes framework support a **plugin** instead of a branch inside the Java
engine.

## The idea

Bennu's core knows Java. It must not also know Spring: the moment "is this bean
ambiguous" lives next to "does this method exist", the two grow into each other and
neither can be changed alone. So a framework is an extension — a self-contained unit handed
the project, answering the editor's questions about it.

Today an extension is a crate the backend links ([`bennu-spring`](../spring) is the
first). Tomorrow it is a WASM module loaded at runtime. This crate is the boundary that
makes those indistinguishable to the caller.

```rust
use bennu_ext::prelude::*;

let registry = ExtensionRegistry::new(vec![Arc::new(SpringExtension::new())], &capabilities);
registry.reindex(&ProjectScan { root, java, xml, resources });   // off the request path

let ctx = FileCtx { path, source };
let diags  = registry.diagnostics(&ctx);
let marks  = registry.gutter(&ctx);
let target = registry.navigate(&ctx, caret_byte_offset);
```

## Four constraints, and why

- **Object-safe, no associated types.** Every method takes and returns plain data, so the
  trait can be implemented by a struct *or* by a proxy forwarding into a WASM instance.
- **The extension owns its model.** `reindex` hands over a project scan; the extension
  keeps whatever it builds behind its own interior mutability. The host stores no
  framework state — nothing to keep in step, and an out-of-process extension keeps its
  model on its own side of the wall.
- **Contributions are wire types.** `ExtHighlight`, `ExtTarget`, `ExtGutterMark`,
  `ExtEntry` are serde types with no Java or Spring concepts in them, so the same values
  travel to the frontend unchanged.
- **Capability-gated.** `ExtensionRegistry::new` keeps only the extensions whose `applies`
  returns true for the project's `CapabilitySet`. A project without Spring never carries
  the Spring extension: no per-query cost, no stray answers — the same rule the UI follows
  when it hides a tool that could only ever be empty.

## What an extension may not assume

It is queried on **any** open file, including ones it has nothing to do with, and possibly
**before** `reindex` has ever run. Every trait method therefore has an empty default and
returning nothing is always correct. Queries also arrive from several threads at once (the
backend dispatches each request on its own thread) — hence `Send + Sync` and `&self`
throughout.

## Catalogs

`catalog(kind)` is the generic backing for every list panel: beans, endpoints, property
keys. One uniform row shape (`ExtEntry`) means one virtualized, filterable list renders all
of them and a new catalog costs no frontend work. Kinds are namespaced by extension id —
`"spring.beans"` goes straight to Spring; a bare `"beans"` is offered to each extension in
turn and the first non-empty answer wins.

## Dependencies

`bennu-proto` only (`Diagnostic`, `CompletionItem`, `CapabilitySet`). Not `bennu-java`, not
`bennu-index`: this crate is the boundary, not an implementation, and an extension that
needs the Java model brings its own parser.
