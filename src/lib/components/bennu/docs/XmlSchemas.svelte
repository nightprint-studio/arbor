<script lang="ts">
  /**
   * XML schemas: where a document's DTD or XSD is found, what it gives the editor, following it, neighbouring versions, the
   * elements a schema requires, the POM's required fields, and what is never claimed.
   */
  import Callout from '$lib/components/shared/ui/Callout.svelte';
</script>

<span class="eyebrow">Java &amp; JSP</span>
<h1>XML schemas</h1>

<p class="doc-lead">
  An XML file in a Java project is a configuration language whose vocabulary is written down precisely — in the DTD or XSD the document names — and normally nothing reads
  it. Bennu does: open a <code>struts.xml</code>, a <code>web.xml</code>, a <code>pom.xml</code> or a <code>beans.xml</code>, type <code>&lt;</code>, and the elements that
  may go there are listed, each with the schema's own description.
</p>

<h2>Where the schema comes from</h2>
<p>
  A document names its schema by URL, and Bennu never fetches it during a scan. It does not have to: frameworks ship their grammar inside their own jar.
</p>
<table>
  <thead><tr><th>Jar</th><th>Carries</th></tr></thead>
  <tbody>
    <tr><td><code>struts2-core.jar</code></td><td><code>struts-2.5.dtd</code></td></tr>
    <tr><td><code>spring-beans.jar</code></td><td>Every <code>spring-beans.xsd</code> ever published</td></tr>
  </tbody>
</table>
<p>
  So the file the URL names is already on the machine. Schemas kept in the project are found too, and win over a jar copy of the same name. The Maven POM is the one nobody
  ships, so its vocabulary is built in.
</p>

<h2>What you get</h2>
<div class="feature-grid two-col">
  <div class="feature-card">
    <div class="fc-eyebrow">Typing</div>
    <div class="fc-title">Completion</div>
    <div class="fc-desc">Elements filtered by what the parent may contain; attributes minus those written; attribute <em>values</em> where the schema closes the set.</div>
  </div>
  <div class="feature-card">
    <div class="fc-eyebrow">When one thing can follow</div>
    <div class="fc-title">Ghost text</div>
    <div class="fc-desc">Never where the rest of the name is already written — most carets, in a document whose closing tags the editor typed for you.</div>
  </div>
  <div class="feature-card">
    <div class="fc-eyebrow">Resting the pointer</div>
    <div class="fc-title">Hover</div>
    <div class="fc-desc">The schema's documentation, the required attributes, and which grammar answered.</div>
  </div>
  <div class="feature-card">
    <div class="fc-eyebrow"><kbd>Ctrl</kbd> + <kbd>B</kbd></div>
    <div class="fc-title">Go to the declaration</div>
    <div class="fc-desc">On a tag or an attribute, its declaration in the schema — which turns <code>&lt;result type="…"&gt;</code> from a word into something you can read.</div>
  </div>
</div>

<h2>Following the schema itself</h2>
<p>
  <kbd>Ctrl</kbd> + <kbd>B</kbd> on the <code>DOCTYPE</code> or the <code>xsi:schemaLocation</code> opens the grammar the file is actually checked against — the copy out of the
  jar, not the address it is written as.
</p>
<p>
  When nobody ships one, Bennu downloads it once and caches it — and the cached copy joins the catalog, so a <code>pom.xml</code> stops being answered by the built-in table and
  starts being answered by the real Maven schema. Nothing is ever fetched during a scan, only when you follow the link.
</p>
<Callout variant="tip" title="The way to get a grammar at all">
  On a machine with no copy of the Struts DTD a <code>struts.xml</code> has no completion, no checks and no hover — and following the address in its <code>DOCTYPE</code> fixes
  all three at once. Every other answer here needs a grammar first, so this one is offered with or without.
</Callout>

<h2>When only another version is on the machine</h2>
<p>
  A legacy <code>struts.xml</code> declares <code>struts-2.1.dtd</code> and the jar ships <code>struts-2.5.dtd</code> — the same schema, a different digit, and a strict name
  match resolves nothing. The nearest version answers instead: the newest one <em>at or below</em> what the document asked for — an older schema can only offer less than the
  project may write — and the oldest of the newer ones only when there is nothing below. The Schemas list marks it.
</p>
<Callout variant="info" title="Its checks stay off">
  Completion and hover from a neighbouring version cost a keystroke when wrong; an underline read off the wrong schema is an accusation about a document that may be correct.
</Callout>

<h2>The elements a schema insists on</h2>
<p>
  A <code>&lt;servlet&gt;</code> with no <code>&lt;servlet-name&gt;</code>, a Spring <code>&lt;bean&gt;</code> missing what its schema demands, a Struts
  <code>&lt;action&gt;</code> without its <code>&lt;result&gt;</code> — the schema already says these are errors, and Bennu reads it and says so.
</p>
<ul>
  <li>The demand must be unambiguous: where the grammar offers a <em>choice</em> — a servlet may name a class <em>or</em> a JSP — neither side is asked for.</li>
  <li>What the schema wraps in an optional group is not asked for either.</li>
</ul>

<h3>The POM's required fields</h3>
<p>
  A <code>&lt;dependency&gt;</code> with no <code>&lt;artifactId&gt;</code>, a <code>&lt;parent&gt;</code> without its version, a root POM that never says who it is — Maven
  refuses to build them all, and being told at build time is what this is here to stop. The conditional ones are honoured:
</p>
<ul>
  <li><code>&lt;groupId&gt;</code> and <code>&lt;version&gt;</code> are required only when the POM has no <code>&lt;parent&gt;</code>.</li>
  <li>A missing <code>&lt;version&gt;</code> on a dependency or a plugin is never reported — it may come from <code>&lt;dependencyManagement&gt;</code> or a parent this file cannot see.</li>
</ul>

<h2>What it will not do</h2>
<ul>
  <li>Check <em>how many</em> of something is legal, or what belongs in an element's text.</li>
  <li>Say anything without a schema: no grammar means no completion, no ghost text, no warnings — a vocabulary guessed from the tags already there would propose whatever typo
    is already there.</li>
  <li>Check inside content the schema leaves open — <code>ANY</code>, <code>xs:any</code>, a POM <code>&lt;configuration&gt;</code>.</li>
</ul>
<Callout variant="info" title="Prefixed names are never reported">
  A document mixing four namespaces usually has schemas for one of them, and the rest must be invisible rather than wrong. An element <em>containing</em> a prefixed child is left
  alone too, since the namespace nobody can read here may supply exactly what looks missing.
</Callout>
