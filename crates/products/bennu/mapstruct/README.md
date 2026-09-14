# bennu-mapstruct

MapStruct support for Bennu, as a **framework extension** on the [`bennu-ext`](../ext) seam.

```rust
use bennu_mapstruct::prelude::*;
use bennu_ext::prelude::*;

let ext = MapStructExtension::new();
ext.reindex(&ProjectScan { java: &sources, xml: &poms, ..ProjectScan::empty(root) });

ext.diagnostics(&ctx);              // unknown / duplicate / conflicting / unmapped
ext.intentions(&ctx, caret, &probs); // ignore the unmapped ones, fix a typo
ext.completions(&ctx, caret);        // property names inside target = "…" / source = "…"
ext.navigate(&ctx, caret);           // Ctrl+B on a path segment → the property
ext.gutter(&ctx);                    // mapping method → its generated implementation
ext.catalog("mappers");              // the Mappers panel
```

## The idea

A mapper is an interface full of methods nobody writes. What they do is decided by the annotation
processor at build time, and every mistake surfaces as a compiler message about generated code — or,
for the worst one, as no message at all: a target property nothing maps is a *warning*, and the field
arrives `null` in production.

All of it is answerable from the sources, in the file where the mapping is written, while it is being
typed.

## What it checks

| Code | Severity | When |
|---|---|---|
| `mapstruct.unknown-target-property` | error | a `target` path segment names no property of a type known completely |
| `mapstruct.unknown-source-property` | error | the same for `source`; with several source parameters only a path that starts with a parameter's name |
| `mapstruct.duplicate-target` | error | two `@Mapping`s with the same `target` on one method |
| `mapstruct.conflicting-mapping` | error | `source`, `constant`, `expression`, `defaultValue`, `defaultExpression` combined the way the processor refuses |
| `mapstruct.unmapped-target` | warning, or error under `ReportingPolicy.ERROR` | writable target properties that are neither targeted nor implicitly mapped from a same-named source property or parameter |

Fixes (Alt+Enter): `mapstruct.ignore-unmapped` writes one `@Mapping(target = "x", ignore = true)` per
unmapped property above the method (importing `org.mapstruct.Mapping` when needed);
`mapstruct.did-you-mean` replaces an unknown segment with the one property within edit distance 2.

## What it will not judge

Silence is the answer whenever the question is not fully visible:

- **A type the project does not declare** — from a jar, from `java.*`, or declared twice.
- **A type with a supertype outside the project**, or implementing an interface other than
  `Serializable`/`Cloneable`/`Comparable` (a `default` getter could be anywhere).
- **Accessors this crate cannot name**: Lombok `@Accessors`, `@SuperBuilder`, `@With`, a non-public
  access level, fluent setters, a hand-written builder, several constructors with no no-arg one.
- **A policy it cannot read**: a constant, a `config` class it cannot find, a `mappingInheritanceStrategy`,
  or a build with no `pom.xml` (a Gradle build's processor options are not visible). The default is
  MapStruct's `WARN` only when a pom exists and sets no `-Amapstruct.unmappedTargetPolicy`.
- **Methods whose mappings come from elsewhere**: `@InheritConfiguration`, `@InheritInverseConfiguration`,
  `@BeanMapping(ignoreByDefault = true)`, `resultType`/`qualifiedBy`, a wildcard `"."`, a `target` that is
  not a plain literal, or **any annotation it does not recognise** (MapStruct supports composed mapping
  annotations).
- **Collection, array, map, `Optional`, `Stream` or generic methods.**
- **Certainty levels**: a property Lombok's constructor or builder *may* take is never reported unmapped,
  and an unknown property is one whose name appears **nowhere** on the type — a private field is enough
  to stay silent.

`ignore = true` combined with `source`/`constant`/`expression` is not reported: which MapStruct versions
refuse it is not certain.

## Design notes

**The model answers, the buffer positions.** Reindex builds the type table, the `@MapperConfig`
policies, the build's default policy, the generated implementations and the panel rows. Every editor
query re-reads the buffer for spans.

**Not every file is parsed.** Mapper files are found by `org.mapstruct`; referenced types by name
(`class User`, `record UserDto`), then a few rounds of the types their properties and supertypes name.

**`@Mapping` is read off the tree**, not the facts model: nested `@Mappings({ @Mapping(...) })` loses its
elements there.

**Lombok is `bennu-lombok`'s catalogue** — the import gate decides whether `@Data` is Lombok's.

## Layout

| File | Holds |
|---|---|
| `properties.rs` | a type's properties with read/write certainty, Lombok included |
| `table.rs` | the project type table, name resolution, supertype-merged views |
| `annotations.rs` | `@Mapping` / `@Mappings` / `@BeanMapping` read from the method's syntax tree |
| `mapper.rs` | mappers and mapping methods analysed: target, sources, kind, policy |
| `paths.rs` | dotted paths and the walk along property types |
| `checks.rs` | the five diagnostics |
| `fixes.rs` | the two intentions |
| `editor.rs` | completion, go-to and hover inside a path string |
| `catalog.rs` | the Mappers panel rows |
| `model.rs` | reindex: file selection, configs, default policy, generated implementations |
| `ext.rs` | `FrameworkExtension` impl |

Public API through `bennu_mapstruct::prelude`.
