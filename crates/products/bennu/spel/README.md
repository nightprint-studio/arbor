# bennu-spel

The two little languages that live inside Spring annotation strings, as scanners.

```rust
use bennu_spel::prelude::*;

let p = &placeholders("${db.url:jdbc:postgresql://localhost/x}")[0];
assert_eq!(p.key, "db.url");                       // navigate / look up this
assert!(p.default.is_some());                      // …so it can never be "missing"

let e = &expressions("#{@userService.findAll()}")[0];
assert_eq!(e.bean_refs[0].name, "userService");    // Ctrl+B target
```

## Why it exists

`@Value("${app.timeout:30}")` is, to a Java editor, one opaque string literal. Nothing
colours it, nothing navigates out of it, and a typo in it is invisible until the
application context fails to start. This crate turns those strings into spans:

- **`placeholder`** — `${key}` / `${key:default}`, nestable (`${${platform}.url}`). Gives
  the key span (so go-to and hover target the key, not the default), whether a default
  exists (a placeholder with one can never be unresolved), and whether the braces closed.
- **`spel`** — `#{ … }` tokenized, not evaluated. Bean references (`@svc`), context
  variables (`#root`), type references (`T(java.lang.Math)`), literals, keywords,
  operators — each as a span with a kind.

## What it deliberately does not do

There is no AST, no evaluation, no type checking. The issue lists carry only what is
broken *as a matter of fact* — an unclosed `#{`, an unterminated string, an unbalanced
bracket. "This operator looks wrong" is an opinion, and opinions do not belong in a
squiggle: under-reporting is the project-wide stance (docs §7), and a false positive in
someone's editor costs more than a missed warning.

Two consequences worth knowing:

- **Inline lists close nothing.** `#{{1,2,3}}` is valid, so the expression's closing brace
  is neither the first `}` nor the last. The body scan tracks brace depth *and* string
  state.
- **A composed key is never resolved.** `${${platform}.url}` is marked `nested`, and both
  placeholders are returned; the outer key only exists at runtime, so it must never be
  reported as missing.

## Shape of the API

Every span is a **byte** offset into the string passed in, half-open. The delimiters that
drive both scanners are ASCII, so spans are always valid `str` slice bounds even in text
with accents. Nothing here allocates beyond the returned `String`s.

## Dependencies

None — not even `serde`. This is the crate most ready to become an out-of-process WASM
extension module: everything crossing its boundary is a span and a plain string, so there
is no wire type to version and nothing framework-shaped to keep in step.

Consumed by [`bennu-spring`](../spring), which owns the Spring model these spans are
resolved against.
