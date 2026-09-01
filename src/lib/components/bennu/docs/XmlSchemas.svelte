<!-- Bennu docs — XML with a DTD or XSD behind it: completion and validation from the schema. -->
<h1>XML schemas</h1>
<p class="doc-lead">
  When an XML file declares a DTD or an XSD, Bennu fetches it once and then uses it — completion
  of the elements that are actually allowed there, and a warning about the ones that are not.
</p>

<h2>XML with a schema behind it</h2>
<p>
  An XML file in a Java project is a configuration language whose vocabulary is written down
  precisely — in the DTD or XSD the document names — and normally nothing reads it. Bennu does.
  Open a <code>struts.xml</code>, a <code>web.xml</code>, a <code>pom.xml</code> or a
  <code>beans.xml</code> and typing <code>&lt;</code> lists the elements that may go there, with
  the schema's own description of each.
</p>
<p>
  <strong>Where the schema comes from.</strong> A document names it by URL, and Bennu never fetches
  one. It does not have to: frameworks ship their grammar inside their own jar —
  <code>struts2-core.jar</code> carries <code>struts-2.5.dtd</code>, <code>spring-beans.jar</code>
  carries every <code>spring-beans.xsd</code> ever published — so the file the URL names is already
  on the machine. Schemas kept in the project itself are found too, and win over a jar copy of the
  same name. The Maven POM is the one exception nobody ships, so its vocabulary is built in.
</p>
<p>
  <strong>What you get.</strong> Element names filtered by what the parent may contain; attribute
  names, with the ones already written removed; attribute <em>values</em> where the schema closes
  the set. Ghost text where exactly one thing can follow — and never where the rest of the name is
  already written, which is most carets in a document whose closing tags the editor typed for you.
  Hover with the schema's documentation,
  the required attributes, and which grammar answered. <kbd>Ctrl</kbd> + <kbd>B</kbd> on a tag or
  an attribute jumps to its declaration in the schema — which turns
  <code>&lt;result type="…"&gt;</code> from a word into something you can read.
</p>
<p>
  <strong>Following the schema itself.</strong> <kbd>Ctrl</kbd> + <kbd>B</kbd> on the
  <code>DOCTYPE</code> or the <code>xsi:schemaLocation</code> opens the grammar the file is
  actually checked against — the copy out of the jar, not the address it is written as. When
  nobody ships one, Bennu downloads it once and caches it, and that is worth more than the
  reading: the cached copy joins the catalog, so a <code>pom.xml</code> stops being answered by
  the built-in table and starts being answered by the real Maven schema. Nothing is ever fetched
  during a scan — only when you follow the link.
</p>
<p>
  <strong>The elements a schema insists on.</strong> A <code>&lt;servlet&gt;</code> with no
  <code>&lt;servlet-name&gt;</code>, a Spring <code>&lt;bean&gt;</code> missing what its schema
  demands, a Struts <code>&lt;action&gt;</code> without its <code>&lt;result&gt;</code> — the
  schema already says these are errors, and Bennu reads it and says so. The demand has to be
  unambiguous: where the grammar offers a <em>choice</em> — a servlet may name a class
  <em>or</em> a JSP — neither side is ever asked for, and anything the schema wraps in an
  optional group is not asked for either. What it will not say is <em>how many</em> of something
  is allowed.
</p>
<p>
  <strong>The POM's required fields are checked.</strong> A <code>&lt;dependency&gt;</code> with no
  <code>&lt;artifactId&gt;</code>, a <code>&lt;parent&gt;</code> missing its version, a root POM
  that never says who it is — Maven refuses to build all of them, and being told at build time is
  what this is here to stop. The conditional ones are honoured: <code>&lt;groupId&gt;</code> and
  <code>&lt;version&gt;</code> are only required when the POM has no <code>&lt;parent&gt;</code>,
  and a missing <code>&lt;version&gt;</code> on a dependency or a plugin is never reported — it may
  come from <code>&lt;dependencyManagement&gt;</code> or from a parent this file cannot see.
</p>
<p>
  <strong>What it will not do.</strong> Check <em>how many</em> of something is legal, or what
  belongs in an element's text. Nor will it say anything at all
  without a schema. No grammar resolved
  means no completion, no ghost text and no warnings — a vocabulary guessed from the tags already
  in the file would confidently propose whatever typo is already there. And where a schema says
  content is unconstrained (<code>ANY</code>, <code>xs:any</code>, a POM
  <code>&lt;configuration&gt;</code>) nothing inside is checked. Prefixed names are never reported
  either: a document mixing four namespaces usually has schemas for one of them, and the rest must
  be invisible rather than wrong — and an element that <em>contains</em> a prefixed child is left
  alone for the same reason, since the namespace nobody here can read may be supplying exactly
  what looks missing.
</p>
