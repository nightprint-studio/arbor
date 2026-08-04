# bennu-jpa

JPA and Spring Data support for Bennu, as a **framework extension** — the second implementation of
the [`bennu-ext`](../ext) seam, and the one that paid for extracting [`bennu-facts`](../facts).

```rust
use bennu_jpa::prelude::*;
use bennu_ext::prelude::*;

let ext = JpaExtension::new();               // applies only where persistence is on the classpath
ext.reindex(&ProjectScan { root, java, xml, resources, descriptors });

ext.diagnostics(&ctx);       // derived-name typos, unbound :params, argument counts
ext.gutter(&ctx);            // entity ⇄ repository
ext.catalog("entities");     // the Entities panel
```

## What it knows

| Piece | Source |
|---|---|
| **Entities** | `@Entity` / `@Embeddable` / `@MappedSuperclass`, with `@Table`, `@Id`, `@Column`, `@Transient` and the four relations. Both `javax.persistence` and `jakarta.persistence` |
| **Inheritance** | a `@MappedSuperclass`'s fields folded into every child — the `id` most derived queries address |
| **Repositories** | recognised by **what they extend**, because a Spring Data interface usually carries no annotation at all |
| **Declared queries** | the JPQL or SQL inside `@Query`, tokenized, with its `:named` and `?1` placeholders |
| **Derived queries** | the method *name*, parsed into subject, predicate, keywords and ordering, and resolved against the entity |

## Why the derived names are the point

`findByCustomerNameAndTotalGreaterThan` is compiled by Spring Data at **application start**. A typo
in one is invisible to the compiler, invisible to every test that does not touch that repository,
and then it takes the whole context down on deploy with `No property 'custmer' found for type
Order`. Resolving every segment against the entity model is the single most valuable thing here.

Resolution is **greedy, like Spring's own**: the whole segment is tried as one property before any
split, so a literal `customerName` field wins over a `customer.name` traversal when both exist —
exactly as at runtime.

## Never a false positive

Every check is gated so that silence is the default:

- an entity whose `@MappedSuperclass` chain leaves the project → the property check is **off** for
  it entirely (a jar's `AbstractPersistable` would otherwise flag forty entities at once);
- a relation whose target was never scanned → the path stops, unverified rather than wrong;
- a repository over a type we do not have → nothing is checked;
- the argument-count check only runs when the name resolved cleanly, so one mistake yields one
  complaint.

**Nothing about the database is checked.** Whether the column exists, whether the type matches,
whether the native SQL parses — that needs a connection, which is Picus's business. Claiming
otherwise on a legacy schema nobody has migrated is how a tool starts lying.

## Generation

[`generate`](src/generate.rs) produces **text**; nothing writes to disk, which is also what makes
all of it testable. Each generator returns whichever destinations it can honestly offer — a
projection is genuinely both a file of its own and an interface nested in its repository, and which
is right is a house-style question.

The query builder's guarantee is tested directly: a generated name is fed back through
[`derived::parse`](src/derived.rs) and resolved, so **the generator cannot emit a name its own
checker would reject**.

Where a generated repository goes is *read off the repositories that exist* rather than assumed —
"where do repositories live" is a convention every codebase settles differently, and guessing wrong
puts the file in the wrong package every single time.

## Public API

Through the [`prelude`](src/prelude.rs). The host registers `JpaExtension`; the backend's
`frameworks` domain calls the generators directly, because generating Java source is not a question
about a caret and the seam has no verb for it.
