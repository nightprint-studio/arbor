# bennu-jsp

The **tag-library** half of JSP support: what `<s:iterator>` is, and how the editor knows.

## Why this exists

A legacy page is mostly not HTML and mostly not Java. It is `<s:…>`, `<c:…>`, `<wp:…>` — a
vocabulary defined in `.tld` files that ship inside the frameworks' own jars, listing every tag,
every attribute, which of them are required, and carrying the prose the framework's website
reprints. Until this crate, none of it was read: the editor had nothing to complete, nothing to
check, nothing to say on hover, and no way to reach the file that would have answered.

## Layout

| File | Holds |
|---|---|
| `tld.rs` | the TLD model + its parser — one shape for the 1.2 and 2.1 generations |
| `directives.rs` | the page's `<%@ taglib %>` declarations, with spans |
| `catalog.rs` | which library a `uri="…"` means, and where its TLD is |
| `intel.rs` | completion, checks, hover and go-to over the two |
| `ext.rs` | the `FrameworkExtension` impl the backend registers |

## One parser for two TLD generations

The 1.1/1.2 form is a DTD document (`<taglib><tag><name>…`), the 2.0/2.1 form is namespaced XSD
(`<taglib xmlns="http://java.sun.com/xml/ns/j2ee">`), and the difference is entirely in the
envelope: the element names that matter are identical. Every lookup goes through the **local**
name, which is what lets one parser read both without a version switch. The same applies inside a
declaration — `<required>yes</required>` and `<required>true</required>` are the same statement
written eleven years apart, and both are in the same project.

## Three registers for one question

`uri="…"` is not a lookup, because a page declares its libraries three different ways and all
three appear in the same file:

```jsp
<%@ taglib prefix="s"  uri="/struts-tags" %>                        <!-- the TLD's own <uri> -->
<%@ taglib prefix="wp" uri="aps-core.tld" %>                        <!-- a file, by name -->
<%@ taglib prefix="c"  uri="http://java.sun.com/jsp/jstl/core" %>   <!-- a URI, from a jar -->
```

So resolution is a short ladder, most-specific first: the URI a TLD claims for itself, then a
`web.xml` `<taglib-uri>` alias (which exists precisely to override the first), then the path —
because `aps-core.tld` and `/WEB-INF/tld/aps-core.tld` are the same declaration written two ways
and a container resolves both. The path rule is a **segment-aligned** suffix match, which is what
keeps `core.tld` from resolving to `aps-core.tld`.

Nothing here reads the filesystem: TLDs arrive as text, from the project or from a jar entry the
host already extracted, exactly like the schemas in [`bennu-xml`](../xml).

## The tag scan is XML's

A taglib tag *is* an XML tag, and [`bennu-xml`](../xml) already owns a tolerant scanner over a
buffer being typed plus the rule that says which of the four caret positions you are in. Writing
a second one would be writing the same bugs again. A JSP's own constructs are invisible to it — a
`<%` is not a tag name, so scriptlets and directives are simply not tags — which is exactly the
behaviour wanted here; the directives are found separately, by `directives.rs`, because they are
the one construct this crate needs and XML cannot see.

The **JSP grammar proper** lives in the frontend (a tree-sitter grammar compiled to wasm, in
`../jsp-grammar`), so nothing here parses a page.

## Nothing is reported without the library that would know

Every check is gated the way the schema checks next door are, and for the same reason — a false
report costs more than a missed one:

- **no libraries resolved at all** (dependencies not resolved yet) → silence, not a page full of
  warnings;
- **a prefix the page does not declare** → never reported, because a legacy page inherits its
  prefixes from an included fragment more often than not, and the include is invisible from here;
- **a tag declaring `<dynamic-attributes>`, or a tag file** → its attribute list is *unknown*,
  not empty.

## Not here

The EL / OGNL AST and the value-stack resolver. `bennu-web` carries the Struts half of that
today; when it moves, it moves here.
