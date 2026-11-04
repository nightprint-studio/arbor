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
  actually written in — <strong>Project Configuration → Encoding</strong>, or the footer's encoding picker for one file
  (see <a href="#reinterpret">Reinterpret</a> below).
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

<h2 id="reinterpret">Reinterpret: read it differently, change nothing</h2>
<p>
  The encoding pill in the editor's footer — or <strong>Reinterpret file as encoding…</strong> in the palette — makes Bennu
  <em>read</em> the open file as another encoding. The file on disk is not touched; the choice is remembered for that file in
  your profile, and <strong>As the project reads it</strong> takes it back. Use it when accents show as <code>Ã©</code> or a
  replacement glyph: the bytes are fine, they were just read with the wrong rule.
</p>

<h2>Convert: rewrite the file</h2>
<p>
  <strong>Convert file encoding…</strong> (palette, the footer pill's last entry) rewrites the bytes in another encoding.
  <strong>Convert project encoding…</strong> and the tree's <strong>Convert encoding…</strong> on a folder or module do the same for
  a whole subtree. The dialog previews as you change options, and nothing is written until you press the button
  (<kbd>Ctrl</kbd> + <kbd>Enter</kbd>):
</p>
<ul>
  <li><strong>From</strong> is, by default, each file's own encoding as Bennu reads it. Set it explicitly when the preview marks
    files <em>Not valid</em> — it then suggests what they look like.</li>
  <li><strong>Byte-order mark</strong> (UTF-8 and UTF-16 targets): keep what the file had, add one, or remove it.</li>
  <li><strong>A character that does not fit</strong> — the euro sign has no Latin-1 byte, an emoji has no Cp1252 one: stop (nothing is
    written, and the preview lists every character and where it is), replace it with <code>?</code>, or write an escape
    (<code>\uXXXX</code> in Java and properties files, <code>&amp;#xXXXX;</code> in XML and JSP).</li>
  <li><strong>Line endings are never changed</strong>, and binary files and files that are not valid in their source encoding are
    skipped and listed rather than damaged.</li>
  <li>A file that declares its own encoding — an XML prolog, a JSP <code>pageEncoding</code> — has it updated, unless you turn that off.</li>
  <li><strong>Declare it in the pom</strong> (whole-project conversions): sets <code>project.build.sourceEncoding</code> so the build
    and Bennu agree with the files.</li>
</ul>
<p>
  Many files run as a <strong>Job</strong> with progress, and can be cancelled from the Jobs panel. Before a file is rewritten it
  is copied to a backup folder in your profile and recorded in Local History, grouped as one change. <strong>Undo
  conversion</strong> (in the dialog afterwards, or <em>Undo last encoding conversion</em> in the palette) puts back every file
  that has not been edited since, and the way Bennu reads them with it.
</p>
<Callout variant="warning" title="Converting changes how Bennu reads the file, too">
  Converting part of a project pins each converted file to its new encoding (so the editor keeps showing it correctly);
  converting the whole project changes the project's encoding instead. Local History keeps older revisions as they were
  written, so a revision from before the conversion is read with the new rule.
</Callout>
