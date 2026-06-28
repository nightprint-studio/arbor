<!-- Merula docs — Editing & refactoring. Plain semantic HTML; DocsShell supplies the
     typography. -->
<h1>Editing &amp; refactoring</h1>
<p class="doc-lead">
  The editor is IDE-grade: completion, go-to-declaration, refactors and quick-fixes, all driven
  from the same language model as the rest of the studio.
</p>

<h2>Writing</h2>
<ul>
  <li><strong>Autocomplete</strong> — <kbd>Ctrl</kbd> + <kbd>Space</kbd> completes islands, combinators, transforms, signals and instrument names, with hover docs from the language reference.</li>
  <li><strong>Go to declaration</strong> — <kbd>Ctrl</kbd> + click a name to jump to its <code>let</code> / <code>fn</code> / <code>import</code> (cross-file too); <kbd>Ctrl</kbd> + click an <code>inst("…")</code> / <code>s("…")</code> opens it in the Preview panel.</li>
  <li><strong>Live diagnostics</strong> — parse errors and unknown instruments underline as you type; out-of-scale notes and clip-risk levels underline too (hover for why).</li>
</ul>

<h2>Navigating</h2>
<ul>
  <li><strong>Find usages</strong> (<kbd>Alt</kbd> + <kbd>F7</kbd>) — every reference to the symbol under the caret, in a floating list (<kbd>↑</kbd> / <kbd>↓</kbd> to move, <kbd>Enter</kbd> to jump).</li>
  <li><strong>File structure</strong> (<kbd>Ctrl</kbd> + <kbd>F12</kbd>) — a filterable jump to any track, function or constant.</li>
  <li><strong>Go to line</strong> (<kbd>Ctrl</kbd> + <kbd>G</kbd>).</li>
</ul>

<h2>Refactoring</h2>
<ul>
  <li><strong>Rename</strong> (<kbd>Shift</kbd> + <kbd>F6</kbd>) — renames a declaration and every use at once.</li>
  <li><strong>Extract</strong> (<kbd>Alt</kbd> + <kbd>Shift</kbd> + <kbd>V</kbd>) — lifts the selected pattern into a named <code>let</code> and leaves the name in its place; <strong>Inline</strong> (<kbd>Alt</kbd> + <kbd>Shift</kbd> + <kbd>N</kbd>) is the inverse.</li>
  <li><strong>Format</strong> (<kbd>Alt</kbd> + <kbd>Shift</kbd> + <kbd>L</kbd>) — reflows the file to canonical style.</li>
</ul>

<h2>Context actions</h2>
<p>
  <kbd>Alt</kbd> + <kbd>Enter</kbd> gathers the quick-fixes that apply where the caret is: fix an
  unresolved instrument by picking the closest installed one, transpose a note (or selection) by a
  semitone / octave in place, snap an out-of-scale note to the nearest degree of the enclosing
  <code>.scale("…")</code> (or change that scale, re-spelling its notes), <strong>humanize</strong>
  or <strong>freeze</strong> the selection, and reach rename / extract / inline.
</p>

<div class="callout accent">
  Every action is also in the Command Palette (<kbd>Ctrl</kbd> + <kbd>K</kbd>); the full keyboard
  reference is under <em>Keyboard shortcuts</em>.
</div>
