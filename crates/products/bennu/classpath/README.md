# bennu-classpath

**JVM-free bytecode reading** for Bennu — reads `.class` metadata (with generics)
from pure Rust, no JVM in-process.

**Leaf crate** (docs §10): no Bennu dependencies, only the off-the-shelf bytecode
stack plus a homegrown Signature decoder.

## What it does

- **`.class` metadata** via `cafebabe` — constant pool, methods, fields, the raw
  `Signature` attribute (`meta.rs` → `ClassMeta` / `MemberMeta`).
- **Generic `Signature` decoding** (`sig.rs`) — a homegrown JVMS §4.7.9.1 parser,
  the one piece no Rust crate does (docs §4). Ported verbatim from the proven spike;
  its test cases (`Optional.map`, `List.iterator`, `Map.entrySet`, class signatures)
  are kept.
- **One `ClassSource` trait** (`source.rs`) over three container formats:

  | Impl            | Container                       | Resource path                         |
  |-----------------|---------------------------------|---------------------------------------|
  | `DirSource`     | dir of `.class` (`target/…`)    | `<root>/java/util/Optional.class`     |
  | `JarSource`     | ZIP jar (`rt.jar`, deps)        | `java/util/Optional.class`            |
  | `JimageSource`  | jimage (`lib/modules`, JDK 9+)  | `/<module>/java/util/Optional.class`  |

  A single decode path serves every container — the only difference between them is
  how a binary name becomes bytes.

- **Shared-seam member index** (`members.rs`) — the structured view Bennu's parser
  and completion consume: `TypeRef` (a binary name + its generic `type_args`),
  `Member` (name / kind / return type / params / static / visibility / raw
  signature), `ClassMembers` (superclass + interfaces + methods + fields), and the
  `MemberIndex` trait (`members_of(binary_name)`). Generics stay **structured**, not
  rendered: `List.iterator()` decodes to `Iterator<E>` and `Optional.map(...)` to
  `Optional<U>`, with type variables surfaced as bare-name `TypeRef`s (`E`, `T`, `K`,
  `V`) so a caller can substitute the receiver's type arguments (generics
  carry-through). `SourceMemberIndex` adapts any `ClassSource` into a `MemberIndex`.

- **JDK bootclasspath resolution** (`jdk.rs`) — `resolve_jdk_classpath(version)`
  locates an installed JDK matching the language level (via `JAVA_HOME` +
  `C:/Program Files/Java/*`, matched against each candidate's `release` file) and
  returns a ready `ClassSource`:
  - `"1.8"` / `"8"` → `rt.jar` + `resources.jar` + `ext/*.jar` chained behind
    `MultiSource`.
  - `"9"`+ / `"21"` → the `lib/modules` jimage, probing `java.base` plus the common
    platform modules.

  **Scope**: JDK bootclasspath only. Dependency-jar sourcing from `~/.m2` is out of
  Phase 1 — those layer in later as extra `JarSource`s behind the same trait.

## Risk

`jimage-rs` is `0.0.x` / mono-maintainer (docs §4). It is kept behind `ClassSource`
so it can be vendored or swapped for `ristretto` without touching call sites.

## Usage

```rust
use bennu_classpath::prelude::*;
```
