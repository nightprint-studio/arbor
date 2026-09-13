<script lang="ts">
  /**
   * Encodings: which one a file is read with, and the check that finds text already damaged by the wrong one.
   */
  import Callout from '$lib/components/shared/ui/Callout.svelte';
  import { highlightCode } from '$lib/utils/highlight';
</script>

<span class="eyebrow">Projects</span>
<h1>Encodings</h1>

<p class="doc-lead">
  Legacy Java sources are very often not UTF-8, and reading one as if it were quietly corrupts every accented
  character in it. Bennu reads each file as what it is — and can tell you which files were damaged before you ever
  opened them.
</p>

<h2>Which encoding a file is read with</h2>
<p>
  A legacy project usually says so in its <code>pom.xml</code>:
</p>
<pre><code>{@html highlightCode(`<properties>
    <project.build.sourceEncoding>Cp1252</project.build.sourceEncoding>
</properties>`, 'markup')}</code></pre>
<p>
  Bennu decodes every file with the encoding the pom declares, and the footer shows which one won — so mojibake never
  slips in silently. When more than one answer exists, the most specific wins:
</p>
<ol class="step-list">
  <li>An override for that one file.</li>
  <li>The <code>sourceEncoding</code> the project's pom declares.</li>
  <li><strong>Settings → Java → Default source encoding</strong> — only ever a fallback, for a project that declares
    nothing.</li>
</ol>
<Callout variant="info" title="A Cargo project is always UTF-8">
  Rust source is UTF-8 by the language's own definition, so the default configured for a legacy Java tree never
  reaches it.
</Callout>

<h2>Finding text that is already broken</h2>
<p>
  <strong>Check file for mojibake</strong>, in the command palette, scans the open file for text that was UTF-8 but was
  once read as Windows-1252 and saved that way:
</p>
<table>
  <thead><tr><th>In the file</th><th>Was</th></tr></thead>
  <tbody>
    <tr><td><code>Ã©</code></td><td><code>é</code></td></tr>
    <tr><td><code>â€™</code></td><td><code>’</code></td></tr>
  </tbody>
</table>
<p>
  Each hit is squiggled with a one-click <strong>Replace with «…»</strong> quick fix, and a summary says how many were
  found.
</p>
<h3>The other kind: a byte that could not be read at all</h3>
<p>
  A file written in Cp1252 and decoded as UTF-8 loses the byte rather than garbling it: the editor shows a replacement
  glyph — one character standing where <code>è</code> was written. There is no quick fix and there cannot be one, because
  the byte that would say what it was is gone before the text reaches the editor. Reload the file in the encoding it is
  actually written in — <strong>Project Configuration → Encoding</strong>, or the footer's encoding picker for one file.
  Typing over the glyph saves a file the original byte has already been thrown away from.
</p>
<Callout variant="warning" title="Both are checked while you type, in any text file">
  They ride the ordinary validation, so a message bundle, a code template, a <code>.sql</code> or a README is checked the
  same way a <code>.java</code> is — which is where a legacy tree keeps almost all of its accented text.
</Callout>
<p>
  The palette command and the project-wide scan are still there for asking on purpose. What the command adds is the
  <em>answer out loud</em> — how many of each kind, or “no broken characters found”, which is the one thing a squiggle
  cannot say. It draws squiggles of its own only in a file that validation does not reach.
</p>
<Callout variant="tip" title="Clean accents are never flagged">
  Detection is exact — a table of real corruption sequences, not a guess about which characters look odd — so correct
  accented text stays unmarked.
</Callout>
