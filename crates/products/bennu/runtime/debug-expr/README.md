# bennu-debug-expr

The small expression language Bennu's two debuggers share — Java over JDWP, Rust over DAP — for
**watches** and **breakpoint conditions**. Pure: no protocol, no process, no I/O, no dependencies.

```text
order.total > 100 && order.status == Status.ACTIVE
items[i + 1].price % 100 == 0
"n=" + n == label            // Java: + joins strings
*self.head.next != 0         // Rust: * follows a reference
```

## Level 1, and the principle that keeps it small

Anything the evaluator does not do is **refused by name**, with the reason, rather than approximated:
a method call (it would run application code inside a paused program), a cast, `instanceof`, `new`,
a lambda, an assignment, `++`, a bitwise operator. A condition that quietly answers something
adjacent to what you typed swallows the stop, with nothing on screen.

A call **parses** (`Expr::Call`) and is refused when evaluated. `Syntax::calls` and
`DebugTarget::call` are the seam where invoking methods will plug in: a target implementing `call`,
not a new grammar.

## What is in it

| Piece | Role |
|---|---|
| `syntax` | `JAVA` / `RUST`: what a language allows |
| `lexer`, `parser`, `ast` | text → `Expr`; precedence: ternary < `\|\|` < `&&` < `== !=` < `< <= > >=` < `+ -` < `* / %` < unary < postfix |
| `eval` | the evaluator, generic over a `DebugTarget` |
| `ops` | per-language numeric semantics (pure, tested alone) |
| `condition` | a condition must read the program and be boolean |
| `path` | the older watch-path grammar (`a.b[2].c`, `*p`) — what mutation, logpoints and the assistant accept |
| `java_names` | `Status` → the binary class names a JVM could know it by (imports, nesting, package) |

## Numbers

| | Java | Rust |
|---|---|---|
| overflow | `int` / `long` wrap | an error (every integer is a checked `i64`) |
| integer `/` | truncates | truncates; `MIN / -1` is an error |
| `/` or `%` by zero | an error | an error |
| `int` with `double` | promoted | refused |
| `char` | promotes to `int` | compares only with `char` |
| `+` on strings | joins | refused |

## Implementing a target

```rust
use bennu_debug_expr::prelude::*;

struct MyTarget { /* a stopped program */ }

impl DebugTarget for MyTarget {
    type Ref = u64; // whatever names an object for you; compared with == for identity
    fn variable(&self, name: &str) -> Result<Option<Val<u64>>, String> { /* … */ }
    fn field(&self, owner: &u64, name: &str, of: &str) -> Result<Val<u64>, String> { /* … */ }
    fn index(&self, owner: &u64, at: i64, of: &str) -> Result<Val<u64>, String> { /* … */ }
    fn deref(&self, pointer: &u64, of: &str) -> Result<Val<u64>, String> { /* … */ }
    // optional: static_field, text (string refs), unbox (Integer → int), call
}

let cond = parse_condition("i + 1 == n", JAVA)?.unwrap();
let holds = eval_condition(&cond, &MyTarget { /* … */ }, JAVA)?;
```

`bennu-be` implements it twice: `debug_jdwp_target` (JDWP) and `debug_dap_target` (DAP variables).
