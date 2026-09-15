# bennu-deps

What a Maven project depends on, and **who decided each answer**.

## The question that makes this worth a crate

A dependency list is easy. `mvn dependency:build-classpath` already produces one, and Bennu
already runs it for the symbol index.

What that list cannot tell you is anything about *why*. In a real project the version is almost
never written next to the dependency that uses it — it is a `${property}` defined three poms up, or
a `<dependencyManagement>` entry in the parent, or nothing at all because something transitive chose
it. "Which version am I actually getting, and which file do I edit to change it" is the question,
and answering it means reading the poms the way Maven reads them.

## The two halves

| Source | Answers |
|---|---|
| the poms (`pom.rs`, `graph.rs`) | modules, scopes, `optional`, profiles, the version and **its origin**, and the byte offset where it is declared |
| the resolved classpath (`repo.rs`) | whether it is really there, and what came in behind it |

Shown next to each other, those two answer the questions a dependency panel is opened for: *why is
this on my classpath*, *why this version*, and *why is this one missing*.

## Structure, not tag-scanning

`<dependencyManagement>` contains `<dependency>` elements that look identical to real ones and are
not dependencies at all — they pin a version for something the module may never use. A reader that
scans for `<dependency>` reports a list twice too long in which the entries that are actually on the
classpath cannot be told from the ones that are not.

So a pom is walked as a tree, by path, using the tolerant scanner from
[`bennu-xml`](../xml) — which also hands over byte spans, and that is what makes every row in the
panel a place the editor can jump to.

## What is deliberately not attempted

Imported BOMs (`<scope>import</scope>`), version ranges, and the conflict mediation that picks
between two transitive versions. Each of those needs the whole repository rather than the files on
disk. Where they are the answer the version stays **empty** rather than guessed — except when the
resolved classpath settles it, which is not a guess: it is the jar the compiler is being handed.

Profile activation is not evaluated either — it depends on the JDK, the OS, a `-P` flag and a
property, none of which an editor knows. A profile's dependencies are reported *and labelled*,
rather than silently included or silently dropped.

## Nothing is executed

No Maven, no network, no build. The classpath comes from whatever the index service already
resolved. A project that has never been built still lists its dependencies correctly; it simply
cannot say which of them resolved, and says so rather than showing them all as missing.

## Consumers

`bennu-be` (`bennu_dependencies`), for the Dependencies tool window.
