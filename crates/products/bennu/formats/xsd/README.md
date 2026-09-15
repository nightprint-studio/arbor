# bennu-xsd

An XML Schema reader. The other half of [`bennu-dtd`](../dtd)'s job, for the grammar that replaced
it: `pom.xml`, Spring's `beans`/`context`/`mvc` namespaces, `web.xml` from Servlet 2.4 on,
`persistence.xml`, `faces-config.xml`.

Uses `roxmltree` — a schema on disk is well-formed by construction, unlike the buffer being
edited (which `bennu-xml` scans with its own tolerant lexer), and it gives byte ranges for free.

## What it produces

An `Xsd`: global elements, named and inline complex types, simple types with their enumerations,
attribute and model groups, and `xs:documentation` attached to whatever it documents. Every
declaration carries a byte offset, so "go to the declaration of this tag" has somewhere to land.

Two resolutions are done for you, because they are XSD's rules and no consumer should have to
know them:

- **Groups are expanded at the point of use.** `xs:group ref` and `xs:attributeGroup ref` become
  the members they name — including when the group is declared *after* the reference, which is
  common and which a single-pass reader gets wrong.
- **Extension chains are walked on demand.** `Xsd::children_of` / `attributes_of` fold in
  everything a type inherits. Nearly every non-trivial schema is a chain of `xs:extension`, and a
  reader that stopped at the derived type would miss most of what a document legally contains.

## Two deliberate simplifications

**Particles are flattened.** `xs:sequence`, `xs:choice` and `xs:all` all become "these elements
may appear here". An editor's questions are *may this element appear inside that one* and *what
may I type here*, and the flattened form answers both. Reconstructing the order would let a
checker report an out-of-order child — and a false one of those is worse than the true ones are
worth, which is the standing rule for every check in bennu.

The names flatten; the **cardinality does not**. `XsdElement::required` is computed during the
walk, not read off `minOccurs` afterwards — by then the `xs:choice` that made three of five names
optional is gone, and every branch of it looks mandatory. The flag survives only a path that
demands it at every step: no multi-branch choice, no `minOccurs="0"` on any wrapper, and not the
head of a substitution group (whose members a document may write instead). It only ever narrows,
so a schema this reader does not fully understand yields *fewer* demands, never invented ones.

**Namespaces are carried, not enforced.** The target namespace is recorded once; matching a
document's prefixes back to it is the consumer's job, because a document mixing four namespaces is
the normal case in Spring XML and the prefix bindings live in the document.

## Not a fetcher

`xs:include` / `xs:import` / `xs:redefine` are **recorded, not followed** (`Xsd::includes`). A
schema arrives as text — from the project, or out of a jar entry — because a parser that opens
sockets cannot run where this one has to. The consumer decides what it can resolve and hands the
next file back in.

## Not a validator

This crate answers *what does the schema say*. Whether a particular document obeys it is
`bennu-xml`'s job — the same split as with DTDs, and what lets one consumer ask both the same
questions.
