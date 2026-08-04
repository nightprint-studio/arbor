# bennu-facts

The Java facts a **framework extension** reads, and the rule for deciding whether an annotation
is the one you think it is. A leaf: tree-sitter and nothing else.

```rust
use bennu_facts::prelude::*;

const MARKERS: &[&str] = &["@Entity", "@Table", "persistence"];
const JPA: AnnotationTable = AnnotationTable::new(&[
    KnownAnnotation { simple: "Entity", packages: &["jakarta.persistence", "javax.persistence"] },
]);

if mentions_any(&source, MARKERS) {                       // cheap pre-filter
    let facts = scan_java(path, &source).unwrap();
    for t in &facts.types {
        if JPA.has(&t.annotations, &facts, "Entity") {     // ...and it is really JPA's
            // t.fields, t.methods, every annotation argument with its byte span
        }
    }
}
```

## Why it exists

`bennu-spring` was the first framework extension and grew two things that turned out not to be
about Spring at all:

- **`scan`** — a tree-sitter pass yielding annotation-shaped facts: types (classes, interfaces,
  enums, **records** — whose components are read as fields), their methods, parameters and
  fields, and every annotation argument with its byte span. Spans are the point: a framework
  lives inside its annotation strings, and highlighting, navigating or linting one means knowing
  exactly where it is.
- **`origin`** — resolving `@Service` / `@Entity` / `@Query` through the file's imports the way
  the compiler would, so a project's own annotation of the same name is never mistaken for the
  framework's.

The second extension needed both, identically. Depending on `bennu-spring` would have been
backwards (JPA exists without Spring) and copying the scanner would have duplicated a file that
keeps changing. So: extraction.

## Mechanism here, policy in the extension

| | Here | In the extension |
|---|---|---|
| Pre-filter | `mentions_any(source, markers)` | which markers |
| Annotation origin | the four-step resolution order | which packages, per name (`AnnotationTable`) |
| Facts | what is written, with spans | what any of it *means* |

The resolution order is the compiler's: written qualified → a single-type import of that simple
name → an on-demand import of an expected package → nothing, which can only be a type in the
file's own package and is therefore rejected. A name **absent** from the table falls back to a
plain name match, so forgetting an entry degrades to the old behaviour rather than to silence.

**Known under-report**: a meta-annotation (a project's `@MyService` that is itself annotated
`@Service`) is not recognised, because that means resolving the annotation's own declaration.
The thing is missed; nothing false is claimed. The right direction when in doubt.

## Public API

Through the [`prelude`](src/prelude.rs): `scan_java`, `mentions_any`, `JavaFacts` / `TypeFacts` /
`MethodFacts` / `FieldFacts` / `ParamFacts` / `AnnFacts` / `AnnString`, `AnnotationTable`,
`KnownAnnotation`, `resolves_to`.

## Consumers

[`bennu-spring`](../spring) and [`bennu-jpa`](../jpa).
