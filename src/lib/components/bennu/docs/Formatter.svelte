<script lang="ts">
  /**
   * The Java formatter: what it does, the layers its style comes from, and every option group.
   */
  import Callout from '$lib/components/shared/ui/Callout.svelte';
  import { highlightCode } from '$lib/utils/highlight';
</script>

<span class="eyebrow">Editor</span>
<h1>Formatter</h1>

<p class="doc-lead">
  Bennu formats Java itself, so it works on code that does not compile and on a file halfway through an edit. Out of the box it
  only re-indents; every rewrite of an expression, a brace or an import is a choice you make in
  <strong>Settings → Java → Formatter</strong>, where a live preview shows what each one does.
</p>

<h2>What it does, and what it does not</h2>
<p>
  The formatter reads the file the way the editor does — as a tree that tolerates errors — and rewrites only whitespace and, where
  you ask for it, line breaks, braces and the order of imports. It never changes what the code means: names, literals and the
  contents of comments and text blocks are left as written, and a file that does not parse still formats.
</p>
<Callout variant="info" title="The default style is the cautious one">
  With nothing changed it re-indents every line to its nesting, strips trailing whitespace, collapses runs of blank lines and ends the
  file with a line break. Spacing normalisation is off, and every wrapping, brace and annotation policy is <em>keep as written</em>.
  Nothing is rewritten that you did not opt into.
</Callout>

<h2>Where the style comes from</h2>
<p>
  A file is formatted with the style that results from laying these layers over one another, later ones winning:
</p>
<ol class="step-list">
  <li><strong>Defaults</strong> — the cautious style above.</li>
  <li><strong>Global</strong> — what you set on the page with <em>Global</em> selected. It applies to every project.</li>
  <li><strong>Detected</strong> — what the project's existing code does nearly always, when <em>Detect from existing code</em> is on.</li>
  <li><strong>.editorconfig</strong> — indent style and width, line width, line ending and final newline from the project's
    <code>.editorconfig</code>, when <em>Read .editorconfig</em> is on (it is by default).</li>
  <li><strong>Project</strong> — the override saved for this project alone.</li>
</ol>
<p>
  Each row on the page says where its value comes from: a small <em>project</em>, <em>editorconfig</em> or <em>detected</em> badge
  marks a value that a layer other than your own style decides, and the reset button on a row undoes what the scope you are editing
  has set.
</p>

<h2>A style for one project</h2>
<p>
  Switch the page to <em>This project</em> and every change is saved as an override of that project only, in
  <code>.arbor/bennu/config.toml</code> under the <code>[format]</code> table:
</p>
<pre><code>{@html highlightCode(`[format]
line_width = 100
wrap_chain = "chop"
brace_method = "next_line"`, 'toml')}</code></pre>
<p>
  Only what differs from the layers below is written: setting an option back to what the global style, <code>.editorconfig</code> or
  detection already say removes it from the file. That file is meant to be committed, so the team formats alike. <em>Reset</em> in
  this scope removes the override altogether.
</p>

<h2>Detecting a project's style</h2>
<p>
  <em>Detect from project…</em> — or <em>Detect code style…</em> in the command palette — measures the project's Java files and
  lists what it found: the indent, where braces go, how long lines get, how chains are wrapped, and how consistently the code
  follows each (<em>91% of 1380</em>). <em>Apply as project style</em> stores the
  result as the project's override, which you can then adjust; <em>Dismiss</em> throws it away. Alternatively, turn on
  <em>Detect from existing code</em> and the measurement is used live, under <code>.editorconfig</code> and the project's override.
</p>

<h2>The options</h2>
<dl class="meta-grid">
  <dt>Indentation</dt>
  <dd>The width of a level (also a tab's display width), tabs instead of spaces, the extra indent of a wrapped line, whether
    <code>case</code> bodies are indented, and whether a wrapped argument list aligns under the character after its parenthesis.</dd>
  <dt>Blank lines</dt>
  <dd>The most consecutive blank lines kept (0 removes them all), and blank lines forced after the <code>package</code> line and
    the imports and before a member. 0 leaves what is written.</dd>
  <dt>Braces</dt>
  <dd>For type declarations, methods, blocks and <code>else</code> / <code>catch</code> / <code>finally</code>: keep the brace where
    it is, put it on the same line, or on the next. <em>Always use braces</em> adds them around a one-statement
    <code>if</code>, <code>else</code>, <code>for</code> or <code>while</code> body.</dd>
  <dt>Spacing</dt>
  <dd>One master switch, <em>Normalize spacing</em>, and fourteen rules under it: before parentheses and braces, around binary
    operators, assignment, ternaries, arrows and the for-each colon, after commas and casts, and inside parentheses, brackets,
    array braces and angle brackets. The rules are inert while the switch is off, and the page shows them dimmed.</dd>
  <dt>Wrapping</dt>
  <dd>The right margin, and a policy for parameters, arguments, call chains, binary expressions, ternaries,
    <code>extends</code> / <code>implements</code> / <code>throws</code> lists and array initialisers. A policy is one of: keep the
    author's breaks; never wrap (join onto one line); wrap if needed (fill each line); chop if needed (one element per line when it
    does not fit); always one per line. A chain wraps only from a minimum number of calls, can keep its first call with the receiver,
    and a wrapped operator can start the new line. A chopped list can put its closing parenthesis on a line of its own and break
    straight after the opening one.</dd>
  <dt>Imports</dt>
  <dd>Sorting and grouping as part of formatting: an ordered list of groups (a package prefix such as <code>java</code> or
    <code>org.springframework</code>, <code>*</code> for everything else, <code>static</code> for the static block), a blank line
    between groups, whether static imports come first, last or mixed in, and the wildcard thresholds.</dd>
  <dt>Annotations</dt>
  <dd>For types, methods and fields: keep, each annotation on its own line, or marker annotations (no arguments) on the declaration's
    line.</dd>
  <dt>Comments</dt>
  <dd>Re-aligning the <code>*</code> of block comments and Javadoc, a space after <code>//</code>, and re-flowing Javadoc prose to the
    right margin.</dd>
  <dt>File</dt>
  <dd>Whether the file ends with a line break, and the line ending written — LF, CRLF, or whatever the file already uses.</dd>
  <dt>Behaviour</dt>
  <dd>Format on save, reading <code>.editorconfig</code>, and detecting from existing code.</dd>
</dl>

<Callout variant="warning" title="Wildcard thresholds can create ambiguities">
  Collapsing a package's imports into <code>package.*</code> changes which names are in scope: two wildcards that both contain a
  <code>List</code> make the simple name ambiguous, and a class added to the package later can shadow another. The thresholds are 0 by
  default — never collapse — and the unused-import check and <em>Optimize imports</em> never create a wildcard. Turn them on only for
  a codebase that already uses them.
</Callout>

<h2>Formatting</h2>
<ul>
  <li><strong>The file</strong> — <kbd>Alt</kbd> + <kbd>Shift</kbd> + <kbd>F</kbd>, or <em>Format file</em> in the command palette.</li>
  <li><strong>The selection</strong> — with text selected, the same key formats just that range; <em>Format selection</em> in the
    palette does it by name. Only the selected lines change, whatever the file around them looks like.</li>
  <li><strong>On save</strong> — with <em>Format on save</em> on, a Java file is formatted when you save it (<kbd>Ctrl</kbd> +
    <kbd>S</kbd>, or when the window is left with auto-save on). Other languages are never touched, the formatting is one undo step,
    and a formatter that fails never prevents the save. The idle auto-save does not format, so half-typed code under your caret is
    left alone.</li>
</ul>
<p>
  A language with a language server — Rust, for example — is formatted by that server, reading the project's own
  <code>rustfmt.toml</code> or <code>.prettierrc</code>; nothing on this page applies to those files.
</p>
