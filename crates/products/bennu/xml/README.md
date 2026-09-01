# bennu-xml

Schema-driven editing for the XML a Java project is made of.

An XML file in a Java project is a configuration language whose vocabulary is written down,
precisely, in a file the editor can read — and almost no editor reads it. So `struts.xml`,
`web.xml`, `pom.xml` and `beans.xml` get edited as prose: you remember the tag names or you look
them up, and a misspelling is found at deploy time.

Everything here follows from taking that grammar seriously.

## Three crates, on purpose

[`bennu-dtd`](../dtd) and [`bennu-xsd`](../xsd) each parse their own format and know nothing of
the other or of this crate. The **unified model and the adapters live here**, which is what keeps
them from having to agree on anything: a DTD parser stays a DTD parser, and "what does an editor
need from a schema" is a question asked in one place.

DTD came first, and not for sentimental reasons — it is the format the files in this codebase
actually declare, its grammar is a quarter the size, and getting the shared model right against
the simpler language meant XSD dropped into a shape that already existed rather than defining it.

## Layout

| File | Holds |
|---|---|
| `scan.rs` | the tolerant scanner over a buffer being typed — tags, attributes, spans, the DOCTYPE |
| `caret.rs` | which of the four positions the caret is in |
| `grammar.rs` | one model behind both languages, plus `from_dtd` / `from_xsd` |
| `catalog.rs` | which schema a document is written against, and where to find it |
| `builtin.rs` | the Maven POM and the JSP tag-library descriptor, because nothing ships their schema |
| `intel.rs` | completion, ghost text, hover, go-to-declaration, checks |
| `ext.rs` | `FrameworkExtension` impl + the grammar cache |

## The rule that makes this work offline

A document names its schema by **URL**. Fetching it is out of the question — an editor that
reaches the network to answer a keystroke hangs on a train — and it is also unnecessary, because
frameworks ship their own schema *inside their own jar*: `struts2-core.jar` contains
`struts-2.5.dtd`, `spring-beans.jar` contains every `spring-beans.xsd` ever published.

So a location is matched by its **file name** against every schema the host could find, in the
project and inside the dependency jars. It is a heuristic, and the failure mode is the acceptable
one: two schemas with the same name resolve to whichever was listed first. Matching the full URL
would resolve nothing at all on a machine that has never been online.

## Nothing is answered without a grammar

No schema resolved → no completion, no ghost text, no diagnostics. Not a guess from the tags
already in the file, which would confidently propose whatever typo is already there.

The checks carry three further gates, each of which exists because breaking it produced a false
report:

- an element the schema declares **open** (`ANY`, `xs:any`, `mixed`) silences everything inside it;
- a **prefixed** name is never reported — a document mixing four namespaces has at most one of
  them resolved, and the rest must be invisible rather than wrong;
- an element whose **parent** the grammar does not know is not judged either: the position is
  unknown, so nothing about it can be.

Cardinality is checked in **one direction only**: a child the grammar is certain a document must
contain and that the document has not written (`Child::required`). Nothing counts *how many* of
something is legal and nothing checks text content — a repeat bound and a text type are things a
grammar records but a flattened or curated one cannot be trusted on.

The certainty comes from the schema readers, not from a guess made here: `bennu-xsd` sets the flag
only on a particle path that demands the name at every step, `bennu-dtd` derives it from the
content-model tree (a choice demands only what *every* branch demands), and the curated tables in
`builtin.rs` never set it at all. Merging two declarations of one name **intersects** the demands
while it unions the names — what a document may contain is the union, what it must contain is the
intersection.

Two gates on top of the three above, both about not knowing enough rather than about being wrong:
`xsi:nil="true"` says the element is deliberately absent, and an element carrying any **prefixed**
child is left alone — a namespace this grammar does not cover may be supplying exactly what looks
missing.

The Maven POM's **required fields** are checked separately, from a curated list in
`builtin.rs` beside the curated vocabulary, for the same reason the vocabulary is there: few,
documented, unchanged since 4.0.0, and not a matter of opinion — Maven refuses to build without
them — and because the real Maven XSD marks nearly all of them `minOccurs="0"`, so the generic
check above cannot find them however well it reads the schema. The list carries no `<version>`
anywhere and excuses `<groupId>`/`<version>` on a POM that has a `<parent>`, because those are the
cases this file cannot know. The check is keyed on the **document**, not on the grammar that
answered, so it holds for a project that vendors the real schema too; where the two can reach the
same field, the duplicate is dropped.

## One name, several declarations

The model is flat: elements are keyed on their local name. A schema does not have to agree —
`plugin` in the Maven POM is `Plugin` under `<build>` and `ReportPlugin` under `<reporting>`, two
different types under one name, and neither is wrong.

So every declaration is walked and the ones sharing a name are **merged**: the entry's children and
attributes are the union of all of them, while its documentation and its declaration site come from
the first seen. Keeping only the first instead cost a real bug — `<executions>` inside an ordinary
`<build><plugins><plugin>` reported as undeclared, because the walk had reached the reporting
declaration first and never descended into `Plugin` at all.

Union is the under-reporting direction, which is the standing rule here: a completion carrying a
name that is legal one level away costs a rejected suggestion; a check that has forgotten half a
declaration reports valid markup as an error.

## Why the scanner is its own

Because the buffer is being typed into, so it is malformed most of the time. `<dependen` is not
well-formed XML and never will be until the word is finished — which is exactly the moment the
user wanted a completion list. A document parser returns nothing there; this scans instead, finds
every tag it can, keeps going past the ones it cannot, and never fails.

## Grammars are cached by what the document asks for

Not by file path. Two hundred `*.hbm.xml` name the same DTD, and parsing it two hundred times per
keystroke would be the whole cost of the feature. The key is the document's *request* — its
`DOCTYPE`, its `schemaLocation`s, its directory and its root element — which also invalidates
itself correctly: change the `schemaLocation` and the key changes.
