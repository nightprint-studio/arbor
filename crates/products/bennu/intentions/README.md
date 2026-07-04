# bennu-intentions

Pure Java **source transforms** behind the Bennu editor's Alt+Enter *intentions* (context
quick-fixes). No tree-sitter, no filesystem, no Tauri — each transform is a byte scanner that takes
`(source, caret_byte_offset)` and returns an edit (`{ start, end, replacement }`), so it is
exhaustively unit-tested here (the frontend has no test runner).

## Intentions

| Function | Does |
|---|---|
| `parameterize_log_call` | `logger.info("user " + id)` → `logger.info("user {}", id)` (SLF4J/Log4j parameterized message; keeps a trailing `Throwable`). |
| `np_safe_equals` | `x.equals("A")` → `"A".equals(x)` (NPE-safe; also `equalsIgnoreCase`). |
| `simplify_size_check` | `x.size()/length() == 0` → `x.isEmpty()` (`!= 0` / `> 0` → `!x.isEmpty()`). |
| `simplify_boolean_compare` | `flag == true` → `flag`, `flag == false` → `!flag` (+ `!=` mirrors). |
| `simplify_negated_comparison` | `!(a == b)` → `a != b`, `!(a != b)` → `a == b`. |

## The aggregation seam

`intentions_at(source, caret) -> Vec<Offer>` runs every transform and returns the applicable ones
(`Offer { id, label, start, end, replacement }`). The editor calls this **once** per Alt+Enter and
renders one item per offer — adding a new intention is a single registration line in `intentions.rs`
(no new IPC/FE wiring).

```rust
use bennu_intentions::prelude::*;

for offer in intentions_at(source, caret) {
    // replace source[offer.start..offer.end] with offer.replacement
}
```

All API is re-exported from `bennu_intentions::prelude`. Shared byte-scanning helpers (paren
matching, string skipping, the postfix-chain backward walk) live in the private `scan` module.
