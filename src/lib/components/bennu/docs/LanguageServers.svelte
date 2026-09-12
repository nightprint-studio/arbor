<script lang="ts">
  /**
   * Language servers: what a server gives a file — the feature list, go to type and symbol, code lenses, hierarchy,
   * placeholders, macro expansion, semantic colouring, and diagnostics on save. Installing one is LspSetup.
   */
  import Callout from '$lib/components/shared/ui/Callout.svelte';
</script>

<span class="eyebrow">Reference</span>
<h1>Language servers</h1>

<p class="doc-lead">
  Bennu's Java intelligence is its own engine. Every other language is served by an external <strong>language server</strong>, and the editor treats both the same way. This page is what a
  server gives you; getting one running is <strong>Installing a language server</strong>.
</p>

<h2>What you get</h2>
<p>For a file a server owns, the whole editing surface is live — the gestures of a Java file, answered by the server:</p>
<table>
  <thead><tr><th>Feature</th><th>How</th></tr></thead>
  <tbody>
    <tr><td><strong>Completion</strong></td><td>As you type and after the language's punctuation — for Rust <code>.</code> and <code>::</code>. An item needing an import brings the <code>use</code> line.</td></tr>
    <tr><td><strong>Go to declaration</strong></td><td><kbd>Ctrl</kbd> + <kbd>B</kbd> or <kbd>Ctrl</kbd>-click; on the declaration itself it flips to find usages, as in Java.</td></tr>
    <tr><td><strong>Find usages</strong></td><td><kbd>Alt</kbd> + <kbd>F7</kbd></td></tr>
    <tr><td><strong>Hover</strong></td><td>The signature, where the item lives, its documentation.</td></tr>
    <tr><td><strong>Diagnostics</strong></td><td>In the editor and the Problems panel — below.</td></tr>
    <tr><td><strong>Rename</strong></td><td><kbd>Shift</kbd> + <kbd>F6</kbd>, with the same preview.</td></tr>
    <tr><td><strong>Quick fixes and refactorings</strong></td><td><kbd>Alt</kbd> + <kbd>Enter</kbd> — the server's whole list: fixes first, then refactorings; one it offers but cannot apply here is shown last <em>with its reason</em>.</td></tr>
    <tr><td><strong>Format</strong></td><td><kbd>Alt</kbd> + <kbd>Shift</kbd> + <kbd>F</kbd>, with the language's formatter and the project's configuration.</td></tr>
    <tr><td><strong>Semantic colouring</strong></td><td>Below.</td></tr>
    <tr><td><strong>Signature help</strong></td><td>The parameters of the call the caret is inside.</td></tr>
    <tr><td><strong>Go to type / symbol</strong></td><td><kbd>Ctrl</kbd> + <kbd>N</kbd>, <kbd>Ctrl</kbd> + <kbd>Shift</kbd> + <kbd>Y</kbd>, across the workspace — below.</td></tr>
    <tr><td><strong>Structure</strong></td><td>The Structure panel and the <kbd>Ctrl</kbd> + <kbd>F12</kbd> popup, in the language's vocabulary — structs, traits, impls, functions.</td></tr>
    <tr><td><strong>Occurrences</strong></td><td>Every other place the symbol under the caret appears in the file, a write tinted apart from a read.</td></tr>
    <tr><td><strong>Folding</strong></td><td>By <em>item</em>: a <code>use</code> block, a doc comment, a <code>#[cfg]</code>-gated module, a match arm.</td></tr>
    <tr><td><strong>Expand / shrink selection</strong></td><td><kbd>Alt</kbd> + <kbd>Shift</kbd> + <kbd>→</kbd> / <kbd>←</kbd>, one syntactic step at a time.</td></tr>
    <tr><td><strong>Placeholders</strong></td><td><kbd>Tab</kbd> through an accepted completion's holes — below.</td></tr>
    <tr><td><strong>Code lenses</strong></td><td>Clickable counts above an item — below.</td></tr>
    <tr><td><strong>Call and type hierarchy</strong></td><td><kbd>Ctrl</kbd> + <kbd>Shift</kbd> + <kbd>H</kbd>, <kbd>Ctrl</kbd> + <kbd>H</kbd> — below.</td></tr>
  </tbody>
</table>
<Callout variant="tip" title="Many refactorings want a selection">
  Extracting a variable or a function has to be told what, so at a bare caret the server's list is short by design. And "selection crosses a block" on a disabled row tells you what to change —
  an absent row would not.
</Callout>

<h2>Go to type, go to symbol</h2>
<p>
  With no Java index, <kbd>Ctrl</kbd> + <kbd>N</kbd> asks the server. The tab is <strong>Types</strong> — structs, enums, traits, unions, aliases — and <kbd>Ctrl</kbd> + <kbd>Shift</kbd> +
  <kbd>Y</kbd>, <strong>Symbols</strong>, finds the rest: functions, methods, constants, statics, fields. The server does the matching, which is why it needs two characters: a workspace is too
  large to hand over, and one character discriminates nothing.
</p>
<p>
  Each row is <strong>named and marked in the language's vocabulary</strong>. The protocol has 26 fixed kinds and every language squeezes in — rust-analyzer reports a <code>trait</code> as an
  interface, an <code>impl</code> as an object, an alias as a type parameter — so they are translated back. In Types every row is a type, so the mark is a lettered ring:
</p>
<table>
  <thead><tr><th>Mark</th><th>Is</th></tr></thead>
  <tbody>
    <tr><td><strong>S</strong></td><td>struct</td></tr>
    <tr><td><strong>T</strong></td><td>trait</td></tr>
    <tr><td><strong>E</strong></td><td>enum</td></tr>
    <tr><td><strong>A</strong></td><td>type alias</td></tr>
  </tbody>
</table>
<p>
  In Symbols the mark is a shape, the distinction worth drawing being function against constant against field. A <code>union</code> arrives as a struct and a <code>static</code> as a constant,
  indistinguishable downstream, so both keep the protocol's word rather than a guess.
</p>

<h2>Code lenses</h2>
<p>
  A line above an item counting <strong>implementations</strong> of a trait, or <strong>references</strong> to a type or trait, and taking you to them — one result jumps, several open the
  <kbd>Alt</kbd> + <kbd>F7</kbd> popover. It costs nothing extra: the server found those places to count them and sends the list along. Reference counts are asked on types and traits only —
  each is a query per item, and methods are by far the most numerous.
</p>

<h2>Call and type hierarchy</h2>
<p>
  <kbd>Ctrl</kbd> + <kbd>Shift</kbd> + <kbd>H</kbd> on a function opens its <strong>callers</strong>; <kbd>Ctrl</kbd> + <kbd>H</kbd> on a type its <strong>implementors</strong> — in the
  Hierarchy panel, whose direction chip walks the other way. On a file the server owns it answers; on a <code>.java</code> Bennu's own index does, into the same panel.
</p>
<Callout variant="info" title="One level at a time, necessarily">
  Expanding a call graph eagerly does not terminate: mutual recursion, a trait implemented by a type using it, or a widely-called helper turn "expand everything" into a sweep of the workspace.
  A recursive chain is walked as far as you care to.
</Callout>
<p>
  A caller row jumps to the <em>call</em>, not the head of its function, and says <code>3×</code> for several calls inside one. The panel takes the keyboard as it opens: arrows walk and expand,
  <kbd>Enter</kbd> jumps.
</p>

<h2>Placeholders in a completion</h2>
<p>
  Accepting a completion with holes puts the caret in the first and <kbd>Tab</kbd> moves on — <code>println!</code> lands between its parentheses, a function on its first argument.
  <kbd>Shift</kbd> + <kbd>Tab</kbd> goes back, <kbd>Esc</kbd> leaves the run, and moving elsewhere ends it. Two placeholders the server marks as the <em>same</em> value are two ordinary stops,
  not one mirroring the other.
</p>

<h2>Expanding a macro</h2>
<p>
  <kbd>Alt</kbd> + <kbd>Shift</kbd> + <kbd>M</kbd> on a macro call shows what it generates, in a dialog to read, copy and re-expand from. It is also under <kbd>Alt</kbd> + <kbd>Enter</kbd>,
  named after the macro — <em>Expand vec!</em> — only when the caret really is inside a macro call.
</p>
<ul>
  <li>The expansion is <strong>recursive</strong>, all the way down: the server has no single-step form.</li>
  <li>It is <strong>text</strong>, not a file the server knows — no go-to, hover or completion inside. To expand a nested macro, point at it in the real source and expand again.</li>
  <li><strong>Re-expand</strong> asks about the caret as it is now: move it in the file behind the dialog and press it.</li>
</ul>

<h2>Semantic colouring</h2>
<p>A file is coloured twice:</p>
<div class="feature-grid two-col">
  <div class="feature-card">
    <div class="fc-eyebrow">Local and instant</div>
    <div class="fc-title">First pass</div>
    <div class="fc-desc">Keywords, strings, comments, numbers — what you see the moment a tab opens, and when no server is running.</div>
  </div>
  <div class="feature-card">
    <div class="fc-eyebrow">From the server</div>
    <div class="fc-title">Second pass</div>
    <div class="fc-desc">A struct told from a trait, a macro from a function, a <code>mut</code> binding from an immutable one, a parameter from a local — facts rather than guesses.</div>
  </div>
</div>

<h2>Diagnostics arrive on save</h2>
<p>
  For Rust the real errors — types, borrows, unreachable code — come from <code>cargo check</code>, which the server runs when a file is <strong>saved</strong>. They appear a moment after a save;
  while typing, you see what the parser alone can tell. Autosave counts as a save, so with it on — the default — the loop is: stop typing, wait a beat, read the compiler's answer.
</p>
<Callout variant="tip" title="Clippy instead of check">
  <strong>Settings → Rust</strong> chooses the command: <code>cargo check</code>, or <code>cargo clippy</code> — every check error plus several hundred lints, at the cost of a
  slower build per save. The server reads it when it starts, so it applies on the next start.
</Callout>
