<script lang="ts">
  /**
   * Go to class, file, symbol: one navigator over three lists, how names are matched and narrowed, and reaching what is
   * inside the dependencies and the JDK.
   */
  import Callout from '$lib/components/shared/ui/Callout.svelte';
</script>

<span class="eyebrow">Editor</span>
<h1>Go to class, file, symbol</h1>

<p class="doc-lead">
  Reaching something by its name — including the names that are not in your source tree at all.
</p>

<h2>One navigator, three lists</h2>
<table>
  <thead><tr><th>Tab</th><th>Opens with</th><th>Lists</th></tr></thead>
  <tbody>
    <tr><td><strong>Classes</strong></td><td><kbd>Ctrl</kbd> + <kbd>N</kbd></td><td>Every class</td></tr>
    <tr><td><strong>Files</strong></td><td><kbd>Ctrl</kbd> + <kbd>Shift</kbd> + <kbd>N</kbd></td><td>Every file</td></tr>
    <tr><td><strong>Symbols</strong></td><td><kbd>Ctrl</kbd> + <kbd>Shift</kbd> + <kbd>Y</kbd></td><td>Every method and field the project declares</td></tr>
    <tr><td><strong>All</strong></td><td><kbd>Tab</kbd> to it</td><td>The three together, each one's best few under its own heading</td></tr>
  </tbody>
</table>
<p>
  <kbd>Tab</kbd> moves between the tabs without reopening; they sit above the field because they decide what it searches. The field
  has the caret the moment it opens, and <strong>nothing is read until you type</strong> — listing every class of a reactor would be a
  wait paid on every use, for an answer nobody scrolls.
</p>
<p>
  The selected entry is shown <strong>in context</strong> on the right — the declaration with the lines around it, or a file's head —
  coloured. A list of names says where there is an <code>OrderDao</code>; it does not say whether it is <em>the</em> one you meant, and
  on a legacy tree with four classes of that name that is the only question left. <kbd>↑</kbd>/<kbd>↓</kbd> read as you go; nothing
  opens until <kbd>Enter</kbd>, which jumps a class or a symbol straight to its declaration line. A word selected in the editor pre-fills
  the field.
</p>
<Callout variant="info" title="In a Cargo project">
  Files work in any project; Classes and Symbols read the Java index, so a Cargo project gets its types from a language server instead —
  see below.
</Callout>

<h3>Projects a language server answers for</h3>
<p>
  The first tab is <strong>Types</strong> rather than Classes, and its rows come from the server: a struct, a trait, an enum, a TypeScript
  interface. Where a repository is several languages at once, <em>every</em> server rooted there is asked and the answers merged — a Rust
  workspace with a Svelte app inside lists both.
</p>
<Callout variant="warning" title="A server answers once it is running">
  A server starts when you open a file it serves. Search a repository whose <code>.svelte</code> files you have not opened yet and
  Svelte contributes nothing, because nothing Svelte is running.
</Callout>
<p>
  A <strong>Svelte component</strong> has no declaration inside it to point at — the file <em>is</em> the component — so its server
  reports a generated name, <code>BennuSidebar__SvelteComponent_</code>. Bennu shows it under the name you wrote, among the types, where
  somebody looking for a component goes. The functions and stores in its <code>&lt;script&gt;</code> are ordinary symbols, under Symbols.
</p>

<h2>Narrowing</h2>
<dl class="meta-grid">
  <dt>Module</dt>
  <dd>
    On a multi-module project, a dropdown on the header row of Classes and Files. It lists only modules that have something in them, and
    a nested module wins over the parent listing it — a class under <code>modules/core</code> is filed there, not under
    <code>modules</code>. Switching tab clears it. Every row also says where it came from: the <strong>module</strong> for something this
    build compiles, the <strong>project</strong> for a sibling, the <strong>artifact</strong> for something it only depends on.
  </dd>
  <dt>Workspace</dt>
  <dd>The toggle beside the field — the one Find in project has — makes Classes and Files read <strong>every project</strong> of the workspace. A project's class index is built the first time you search it, and reused.</dd>
</dl>

<h3>How a name is matched</h3>
<p>
  By <strong>subsequence</strong>, not substring — the letters appear in order — and the characters that matched are lit in the row, so a
  loose match is legible rather than mysterious:
</p>
<table>
  <thead><tr><th>Typed</th><th>Finds</th></tr></thead>
  <tbody>
    <tr><td><code>agpo</code></td><td><code>AGGIORNAMENTO/POS</code></td></tr>
    <tr><td><code>agg pos</code></td><td>The same, each term matched on its own — for a path that separates them</td></tr>
  </tbody>
</table>
<p>
  Results rank on where the hit lands: the start of a word beats the middle, a run beats a scattered match, a short name beats a long one.
</p>

<h3>Directives</h3>
<table>
  <thead><tr><th>Directive</th><th>Keeps</th></tr></thead>
  <tbody>
    <tr><td><code>in:dao</code></td><td>Paths containing <code>dao</code></td></tr>
    <tr><td><code>ext:java</code></td><td>That extension</td></tr>
    <tr><td><code>sort:new</code></td><td>Everything, most recently modified first</td></tr>
  </tbody>
</table>

<h2>Reaching what is inside the dependencies</h2>
<p>
  The <strong>Source</strong> picker on the header row decides whose code Classes and Files are about: <strong>Project</strong>, the
  outside, or <strong>both</strong>, ranked into one list rather than two tabs for one question. It is how you reach what is on the
  classpath and nowhere in the tree — the framework annotation whose package you cannot remember, the <code>struts-default.xml</code>
  declaring the interceptor stack, the schema an XML file is validated against.
</p>
<div class="feature-grid two-col">
  <div class="feature-card">
    <div class="fc-eyebrow">The outside, for Classes</div>
    <div class="fc-title">Dependencies &amp; JDK</div>
    <div class="fc-desc">Half of what anyone looks up is in the JDK — <code>List</code>, <code>Optional</code>, <code>Path</code> — and it opens on the real <code>.java</code> from the JDK's <code>src.zip</code>.</div>
  </div>
  <div class="feature-card">
    <div class="fc-eyebrow">The outside, for Files</div>
    <div class="fc-title">The dependency jars</div>
    <div class="fc-desc">From Java 9 the JDK is a single image file, with nothing in it a reader would recognise as a file to open.</div>
  </div>
</div>
<p>
  A dependency row is <strong>tinted</strong> and names its <strong>artifact</strong> — a classpath is where four versions of one name live,
  and what you can read is not what you can change. The same picker is on <strong>Find in project</strong>, where <em>Dependencies</em>
  alone is often what you want.
</p>
<Callout variant="tip" title="Searching the dependencies by default">
  <strong>Search the dependencies too</strong> (Settings → Java) moves the <em>default</em> to <em>Project &amp; dependencies</em>. It no
  longer decides whether the classpath is reachable — that is one pick away.
</Callout>
<ul>
  <li>A library <em>class</em> opens like a stack-trace frame does: the real <code>.java</code> when the JDK ships sources or a
    <code>-sources.jar</code> has been downloaded, otherwise the decompiled stub.</li>
  <li>A library <em>file</em> is extracted from the jar and opened read-only, keeping its extension — so an XML still reads as XML.</li>
  <li>Classpath rows are searched as you type rather than listed: hundreds of thousands of entries, nothing fetched without a query. The
    first search after opening a project spends a moment reading the jars, and is instant after that.</li>
</ul>
