# bennu-ssr

Structural search & replace: find code by its **shape**, count it, rewrite it.

A text search knows nothing about the language, so `log.debug("x" + y)` and the same call spread
over three lines are two strings and one construct. This compares *nodes*, so whitespace, line
breaks and interleaved comments take no part — and because every hole is captured, a replacement
can move its parts around, which no textual find/replace can do at all.

## The language

```text
<pattern>
or <pattern>            zero or more alternatives
in <scope>[, <scope>]   optional
group <key>             optional — turns the answer into a table
```

A **pattern is source text with holes in it**, parsed with the same grammar as the subject.
`$x$` matches one node; `$xs...$` matches a run of consecutive siblings and captures the original
bytes, separators included. A placeholder stands in for an identifier while the pattern is
parsed, so it may sit anywhere a name can — and nowhere a whole statement is expected. That limit
is real and is stated rather than worked around.

A hole may carry a constraint after `:` — a type (`com.acme.Order`, `Order+` for subtypes, `*Dao`
as a glob), a grammar node kind (`#string_literal`), a glob over the node's own text (`~get*`),
what the name **denotes** (`@type` for a static access, `@value` for an instance one), the
negation of any of them (`!equals`), or several joined with `&` (`@type & Files`). `~` and the
type globs are **globs, not regexes**: `*` is any run, `|` alternates, everything else is literal,
anchored. `&` binds looser than `!`, and there are no parentheses.

`@type` / `@value` exists because `orders.total()` and `Orders.total()` are the same shape: the
difference is not syntactic, so it cannot be a pattern — only the resolver knows whether a
receiver names a class or a variable.

`use of $m$ on <Type>` is a separate query kind — the shortcut for "every use of a member",
desugared by the caller into the patterns it stands for.

## Two rules that keep a count honest

**A capture used to `group` must be bound by every alternative.** Otherwise the table has rows
with an empty column, and a hole in an aggregate reads as "none" rather than as "this branch
cannot answer". Refused when the query is parsed, naming the branch.

**One place is one hit.** Two alternatives can describe the same bytes; counting that twice
produces a number that is plausible and wrong. De-duplicated by range in `engine`.

## What this crate does not know

**Java.** The grammar arrives as a parameter, exactly as it does for `arbor-syntax` underneath.
Picus points the same code at SQL.

**Names.** Deciding that `svc` is a `com.acme.OrderService` — or that `Svc` is the class itself —
needs a classpath, imports and local inference. So it is a trait, `TypeOracle`, the caller
implements: the tests here hand it a two-line fake, `bennu-be` hands it the resolver. That is what
keeps every rule in here exhaustively testable without a project on disk.

It has **one** question, `denotation_at`, answering `Type(name)` or `Value(type)`, because the two
readings share all their work: resolving `Files` means asking "is there a variable called that?"
and then "is there a class called that?", and both `$x: Files$` and `$x: @type$` fall out of the
same lookup. Two methods would have resolved the same name twice and let the answers disagree.

Its contract has one clause that matters more than the rest: **`None` means *unknown*, never
"no"**. A hit whose type constraint could not be decided is kept and flagged
(`Hit::unresolved`), and `report` counts it in its own column. Dropping it would produce a table
that looks complete and is short by however much the project failed to resolve — on a legacy
tree, the difference between "used 12 times" and "12 I could confirm, 380 I could not read".

**The filesystem.** A `Subject` is a path and a string.

## Modules

| Module | What |
|---|---|
| `query` | the language and its parser |
| `engine` | compiling, matching, constraints, de-duplication, `enclosing` |
| `report` | the table `group` asks for |
| `replace` | the template, its check, and the edits |

Every public item is re-exported through `prelude` — the canonical call-site path.
