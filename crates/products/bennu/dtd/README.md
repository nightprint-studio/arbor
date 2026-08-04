# bennu-dtd

A DTD parser. No dependencies at all.

## Why a DTD parser in 2026

Because the files a legacy Java project opens every day declare one. `struts.xml`,
`struts-config.xml`, `web.xml` before Servlet 2.4, `hibernate.cfg.xml`, every `*.hbm.xml`: all of
them say `<!DOCTYPE … SYSTEM "…dtd">` and none of them say `xsi:schemaLocation`. An editor that
only understands XSD understands the `pom.xml` and nothing else the user has open.

It is also the cheap one — three declaration forms, no namespaces, no imports, no substitution
groups, no type system. About a quarter of the code an XSD parser needs, for most of the value.

## What it reads

| Declaration | Into |
|---|---|
| `<!ELEMENT name content>` | `ElementDecl` + a `Particle` tree for the content model |
| `<!ATTLIST el name type default …>` | `AttListDecl` → `AttrDecl` with its enumeration and `#REQUIRED` |
| `<!ENTITY % name "…">` | `EntityDecl`, **and the expansion is applied** |

Every declaration carries a byte offset into the file, so "go to the definition of this tag" has
somewhere to land. The comment immediately above a declaration becomes its documentation — a DTD
has nowhere else to put any.

## Parameter entities are not optional

A real DTD is written almost entirely in them:

```
<!ENTITY % common "id ID #IMPLIED  name CDATA #IMPLIED">
<!ATTLIST action %common; class CDATA #IMPLIED>
```

A parser that skips expansion sees one attribute and reports the other two as unknown — worse
than not parsing the file, because it is confidently wrong. Expansion runs to a fixed point with
a bounded number of rounds, so a self-referential entity terminates instead of hanging.

Expansion rewrites the buffer, so offsets are recovered by searching the **original** for
`<!KEYWORD` plus the same name. When that fails the offset is 0: landing at the top of the right
file is a fair answer, landing inside an unrelated declaration is not.

## The content model is a tree, not a set

`(a, (b | c)+, d?)` says things a set of child names cannot, and `Particle` keeps all of it.
`Content::child_names()` gives the flattened set for completion; the tree is there for the checks
that need order.

## What it deliberately does not do

- **Validate a document.** This crate answers *what does the grammar say*. Whether a particular
  file obeys it is `bennu-xml`'s job, and keeping the two apart is what lets the same question be
  asked of an XSD.
- **Fetch anything.** A DTD arrives as text — read from the project, or out of a jar entry —
  because a parser that opens sockets cannot run where this one has to.
- **Fail.** A malformed declaration is skipped, not fatal: a DTD is being read *because* an editor
  is open on something, and half a grammar is worth more than none.
