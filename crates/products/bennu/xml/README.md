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

Nothing checks text content or cardinality. The grammar records both, and a flattened or curated
one is exactly the wrong place to be confident about either.

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
