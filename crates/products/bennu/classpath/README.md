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
  returns a ready `ClassSource`. When no exact-major JDK is installed it **falls back to
  the newest installed JDK** (so a Java-8 project still resolves the standard library on a
  machine that only has a modern JDK) — `Err` only when no JDK is installed at all:
  - `"1.8"` / `"8"` → `rt.jar` + `resources.jar` + `ext/*.jar` chained behind
    `MultiSource`.
  - `"9"`+ / `"21"` → the `lib/modules` jimage, probing `java.base` plus the common
    platform modules.

- **Dependency-jar sourcing from `~/.m2`** (`maven.rs`) —
  `resolve_maven_classpath(project_dir, &opts)` runs
  `mvn dependency:build-classpath` for a Maven project, reads the resolved classpath,
  and turns each existing dep jar into a `JarSource`. `MavenClasspathCache` caches the
  result **keyed by pom mtime** (a re-resolve within a session is free until the pom
  changes). `MavenClasspath::augment(jdk)` chains the dep jars **behind** the JDK
  bootclasspath into one `MultiSource` — the JDK is probed first (its core wins over a
  shaded copy in a dep), then the dep jars. So member-access completion reaches
  framework/library types (Spring, servlet, Hibernate, Struts…), not just the JDK +
  project sources.

  Turning a project into a dep-augmented member index:

  ```rust
  use bennu_classpath::prelude::*;
  use std::path::Path;

  let jdk = resolve_jdk_classpath("1.8")?;                 // Phase-1 bootclasspath
  let mut cache = MavenClasspathCache::new();
  let deps = cache.get(Path::new("/path/to/project"), &MavenResolveOpts::default())?;
  let index = SourceMemberIndex::new(deps.augment(jdk));   // JDK + dep jars
  let members = index.members_of("javax/servlet/http/HttpServletRequest"); // Some(_)
  ```

  **Partial failure is non-fatal.** Some deps may live on a private repo and not
  resolve; `build-classpath` can exit non-zero yet still write the classpath it
  *could* resolve. The output file is always read: existing jars become sources,
  non-existent entries are recorded in `MavenClasspath::unresolved`. Only a failed run
  that wrote *no* file at all is surfaced as an error. A project with no resolvable
  deps degrades exactly to the JDK-only behavior. `MavenResolveOpts` runs Maven
  offline (`-o`) by default and can pin `JAVA_HOME` so the project's JDK is used.

## Risk

`jimage-rs` is `0.0.x` / mono-maintainer (docs §4). It is kept behind `ClassSource`
so it can be vendored or swapped for `ristretto` without touching call sites.

## Usage

```rust
use bennu_classpath::prelude::*;
```
