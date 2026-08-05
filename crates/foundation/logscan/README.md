# arbor-logscan

A log interpreter. A line of program output goes in; what level it is, and what its parts
*are*, comes out.

```rust
use arbor_logscan::prelude::*;

let mut reader = LogReader::new(RuleSet::java());
let line = reader.read("2026-08-05 12:33:01 ERROR [main] com.acme.Boot - see https://acme.test/x");
// line.level == Some(Level::Error)
// spans: timestamp · level · thread · package · url (linked)

let frame = reader.read("\tat com.acme.Order.total(Order.java:118)");
// frame.level == Some(Level::Error)   ← inherited: a frame says nothing about its severity
// frame.links() → Link::Source { class: "com.acme.Order", file: "Order.java", line: 118 }
```

## Why a crate

A console that renders output as a wall of identical grey text makes you read every line to
find the one that matters. Every IDE therefore interprets that text — the level is coloured,
the timestamp and the thread recede, a stack frame becomes a link to the line it names. It is
a real feature and it is entirely mechanical, so it belongs somewhere reusable rather than
inside whichever console needed it first.

## What it recognises

`RuleSet::common()` — everything a log has, whatever produced it:

| Shape | Token |
|---|---|
| `ERROR`, `WARN:`, `[ERROR]`, `SEVERE` | `Level` |
| `2026-08-05T12:33:01.123Z`, `12:33:01,123` | `Timestamp` |
| `[main]`, `[http-nio-8080-exec-3]` | `Thread` |
| `https://…`, `jdbc:…`, `file://…` | `Url` (linked when openable) |
| `/home/u/src/Foo.java:42`, `C:/build/app.jar` | `Path` (linked, line included) |

`RuleSet::java()` adds the JVM:

| Shape | Token |
|---|---|
| `at com.acme.Order.total(Order.java:118)` | `Package` + `Frame`, both linked to the source |
| `com.acme.OrderNotFoundException` | `Exception` |
| `com.acme.order.OrderService` | `Package` |

Every rule is written to **decline** rather than guess. `and/or` is not a path, `Error
handling is enabled` is not an error, `1.8.0_292` is not a package, and `order.total` in a
sentence is not a class — a viewer that highlights those teaches you to ignore its
highlighting, at which point it has made the log harder to read than plain text.

## Two views of the answer

`Line` carries `spans`: byte ranges into `line.text`, which is what a Rust host wants.
`Line::pieces()` returns the text **already cut up**, which is what a host rendering across
an IPC seam wants — no byte offsets cross the wire, because Rust counts UTF-8 bytes and a
JavaScript frontend counts UTF-16 code units, and a range that means two different things on
the two sides is a bug waiting for the first accented log line.

`Link::Source` is deliberately **unresolved**. A stack frame names a class, and only the
host's index can turn a class into a file; resolving it here would make this a Java tool
instead of a log interpreter. It carries the method as well as the class, so a host opening a
view with no usable line numbers — a stub decompiled from bytecode — still has somewhere
precise to land. `outer_class()`, `method_of()` and `is_synthetic()` are provided for hosts
doing that lookup; the last one is how you decline the frames (lambda carriers, proxies,
generated accessors) that have no source anywhere.

## Extending it

A `RuleSet` is an ordered list — first match wins, so it doubles as a priority list. Adding a
rule is a closure:

```rust
use arbor_logscan::prelude::*;

let rules = RuleSet::common().with(FnRule::new("ticket", |text: &str, at: usize| {
    let end = token_end(text, at);
    text[at..end].starts_with("JIRA-").then(|| Hit::one(at, end, Token::Package))
}));
```

A whole new dialect (a Python traceback, a `cargo` diagnostic, an application's own
request-id format) is one module of matchers plus one constructor, and it touches nothing
that already works. `RuleSet::continued_by` says which lines belong to the one above them,
which is what makes a stack trace inherit its error's level.

## What it is not

Not a terminal emulator. SGR (colour, bold) becomes `Style`; cursor movement, erase-line and
the alternate screen are discarded — this produces a transcript of what a program printed,
and a transcript has no cursor to move. A progress bar that redraws itself with `\r`
therefore appears as its successive states, which is the honest rendering of the same bytes.

## Dependencies

`serde`, and only because `Piece` crosses an IPC seam on its way to a frontend — which is the
whole reason it produces pieces rather than markup.
