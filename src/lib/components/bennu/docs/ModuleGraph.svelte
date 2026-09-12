<script lang="ts">
  /**
   * The module graph window: the four questions about a project's shape, how to read the drawing and its lines, the
   * list beside it, solo and search, cycles, exporting, and what a manifest cannot say.
   */
  import Callout from '$lib/components/shared/ui/Callout.svelte';
</script>

<span class="eyebrow">Projects</span>
<h1>The module graph</h1>

<p class="doc-lead">
  The Dependencies panel answers <em>what does this module need</em>. The questions it cannot answer are properties of the
  project's <strong>shape</strong> rather than of any one row — and in a workspace of twenty crates they are the ones you arrive
  with. <kbd>Alt</kbd> + <kbd>Shift</kbd> + <kbd>D</kbd>, or the network button on the Dependencies and Cargo windows, draws them.
</p>

<div class="feature-grid two-col">
  <div class="feature-card">
    <div class="fc-title">Who uses this</div>
    <div class="fc-desc">And therefore whether anything still does.</div>
  </div>
  <div class="feature-card">
    <div class="fc-title">What breaks if I touch it</div>
    <div class="fc-desc">How many modules rebuild, transitively.</div>
  </div>
  <div class="feature-card">
    <div class="fc-title">What is foundational</div>
    <div class="fc-desc">What sits at the bottom, and is worth being careful with.</div>
  </div>
  <div class="feature-card">
    <div class="fc-title">Is there a cycle</div>
    <div class="fc-desc">Which cargo refuses to build and Maven refuses to order.</div>
  </div>
</div>
<p>
  It works on a Maven reactor as well as a Cargo workspace, from the same reading of the manifests. Nothing runs cargo or
  Maven, so it opens on a project that has never been built. The window says <em>crates</em> or <em>modules</em> depending on
  which it is looking at.
</p>

<h2>Reading it</h2>
<p>
  Layers run <strong>left to right: dependents first, the foundation last</strong>, so a chain reads like the sentence that
  describes it:
</p>
<pre><code>app  →  core  →  util</code></pre>
<p>
  An arrow points from the module that <em>declares</em> the dependency to the one it depends on, so every arrow in a healthy
  project points rightwards.
</p>
<Callout variant="info" title="A leftward arrow is a cycle">
  That is why the layout runs this way round: a cycle is visible as a shape, without counting anything or consulting a colour.
</Callout>
<p>
  Each column is labelled with its <strong>layer</strong> — how far above the foundation it sits. Layer 0 depends on nothing
  else in the project; a module is one layer above the deepest thing it depends on. Every module of a cycle shares a layer,
  because there is no order between them.
</p>

<h2>What the lines mean</h2>
<p>
  The <strong>legend</strong> button in the footer draws each mark beside its meaning. It is open the first time you arrive, and
  stays closed once you close it.
</p>
<table>
  <thead><tr><th>Line</th><th>Means</th></tr></thead>
  <tbody>
    <tr>
      <td><strong>solid</strong></td>
      <td>An ordinary dependency — a Cargo <code>dependencies</code> or <code>build-dependencies</code> entry, a Maven
        <code>compile</code>, <code>provided</code> or <code>runtime</code> scope. <strong>It orders the build.</strong></td>
    </tr>
    <tr>
      <td><strong>dashed</strong></td>
      <td>A Cargo <code>dev-dependency</code> or a Maven <code>test</code> scope. Real, and it does <em>not</em> order the build.
        Under cargo it may <strong>legally close a cycle</strong>: a crate's tests compile as a separate unit, so a library whose
        tests use something that depends on it is normal. Maven refuses such a cycle anyway, because it orders the whole reactor as
        one graph. Either way it counts towards <em>what rebuilds</em> — changing the library does rebuild the tests that use it.</td>
    </tr>
    <tr>
      <td><strong>dotted</strong></td>
      <td>Optional — on the graph only when a feature turns it on. Whether one has is not a fact about the manifest, so it is
        drawn and labelled rather than guessed at.</td>
    </tr>
    <tr>
      <td><strong>red</strong></td>
      <td>Part of a <strong>cycle</strong>: the build tool refuses this.</td>
    </tr>
    <tr>
      <td><strong>blue</strong></td>
      <td>Touching the selected module — or the one under the pointer. Resting on a box lights its own lines: the quickest way to
        answer "which of these is mine".</td>
    </tr>
  </tbody>
</table>
<Callout variant="tip" title="The one distinction to remember">
  <strong>Solid dependencies decide the order things are built in; dashed ones do not.</strong> That is exactly why the cycle
  check ignores dashed lines under cargo and not under Maven.
</Callout>
<dl class="meta-grid">
  <dt>The bar on a box</dt>
  <dd>What the module builds: a library, a program, both, a proc-macro; a jar, a war, or an aggregator pom that builds nothing.</dd>
  <dt>The number</dt>
  <dd>How many modules rebuild when it changes.</dd>
  <dt>The border</dt>
  <dd>Red when it is in a cycle, blue when it is selected.</dd>
</dl>
<p>
  An edge that crosses several layers is <strong>routed between the boxes</strong> of the columns it passes rather than straight
  across them — a long dependency you can follow, instead of a line disappearing under three other modules.
</p>

<h2>The list, and the numbers</h2>
<p>
  Beside the drawing every module is a row, and it is not a lesser view: finding a half-remembered crate in a picture of forty
  boxes is a scan, and here it is three keystrokes. It is also the <strong>keyboard surface</strong> — <kbd>↑</kbd> <kbd>↓</kbd>
  walk it, <kbd>Enter</kbd> opens the manifest — so the whole window works without touching the graph.
</p>
<ul class="prop-list">
  <li><strong>Most rebuilt on</strong>how many modules a change here reaches — the number to know before touching something, and why a leaf with a high one is not really a leaf</li>
  <li><strong>Layer</strong>deepest first, in the drawing's order</li>
  <li><strong>Most third-party</strong>which module pulls in the most outside code</li>
  <li><strong>Name</strong></li>
</ul>
<p>
  Selecting is one act across all three views: the list moves the drawing, the drawing moves the list, and either fills the detail
  panel underneath — the module's numbers, and its direct dependencies and dependents as rows you can walk into.
</p>
<Callout variant="info" title="Used by: nothing">
  A fact, not a verdict. A library published to a registry and a deployable war both legitimately have nothing in the project
  depending on them; in a private workspace a library in that state is usually dead code. Only you know which.
</Callout>

<h2>Solo, and searching</h2>
<div class="feature-grid two-col">
  <div class="feature-card">
    <div class="fc-eyebrow"><kbd>Alt</kbd> + <kbd>S</kbd> — a filter</div>
    <div class="fc-title">Solo</div>
    <div class="fc-desc">
      Draws only the selected module's world. The columns are recomputed from what is left and empty ones collapse, so in a workspace
      of sixty crates the other fifty stop taking room. Three scopes: everything connected, only what it is <strong>built on</strong>,
      or only what it would <strong>break</strong>.
    </div>
  </div>
  <div class="feature-card">
    <div class="fc-eyebrow">A dimming</div>
    <div class="fc-title">Search</div>
    <div class="fc-desc">
      The matches stay lit and everything else recedes without moving. Both can be on at once — the search then dims inside the soloed world.
    </div>
  </div>
</div>
<p>
  Solo follows the selection, so picking another module isolates around that one: that is how you walk a dependency chain a crate at
  a time without leaving the mode. The header counts what is on screen — <em>4 of 22 crates</em> — because the project's totals would
  describe a picture you are not looking at.
</p>

<h2>Cycles</h2>
<p>
  The header counts the cycles, and pressing the count goes to one. The whole <strong>ring</strong> is named, not the single pair the
  build tool mentions when it refuses: five crates that all reach each other usually contain several rings, and naming one would
  suggest the others are fine.
</p>
<p>
  What counts as a cycle is the build tool's own answer — see the dashed row above — and the drawing agrees with the count by
  construction: the members share a layer, so their arrows are the ones pointing the wrong way.
</p>

<h2>Taking it elsewhere</h2>
<p>
  The <strong>export</strong> button beside the window's ✕ copies the graph or writes it to a file. There is no import: the manifests
  are the truth, and this is a description of them.
</p>
<div class="feature-grid">
  <div class="feature-card">
    <div class="fc-eyebrow">For a language model, or a person</div>
    <div class="fc-title">Markdown</div>
    <div class="fc-desc">
      Each module with what it depends on and what depends on it, then the three lists worth having: most expensive to change, nothing
      depends on them, the cycles. It <em>says</em> what each number means — fewer tokens than a schema the reader has to infer.
    </div>
  </div>
  <div class="feature-card">
    <div class="fc-eyebrow">For a script</div>
    <div class="fc-title">JSON</div>
    <div class="fc-desc">Every field exactly as computed, keyed by module name rather than by array index.</div>
  </div>
  <div class="feature-card">
    <div class="fc-eyebrow">For a spreadsheet</div>
    <div class="fc-title">CSV</div>
    <div class="fc-desc">One row per edge, with a column saying whether it orders the build.</div>
  </div>
</div>
<Callout variant="warning" title="It exports what is on screen">
  Solo and the dashed-edge filter included, and the file's own header says which filters were on. An export that silently described
  the whole project while the window showed one crate's neighbourhood would mislead whoever — or whatever — read it.
</Callout>
<p>RON is deliberately not offered: it would be a second spelling of the JSON with nothing that prefers to read it.</p>

<h2>What it will not tell you</h2>
<p>
  Everything here is read from the manifests, which is what makes it instant and makes it work on a project that has never compiled.
  The cost is stated rather than hidden: <strong>nothing is resolved</strong>. Feature unification across the workspace, which
  dependencies a <code>cfg(…)</code> actually admits, and whether a Maven profile is active are cargo's and Maven's answers, not the
  manifests' — so conditional and optional edges are drawn and labelled.
</p>
<p>
  A project with more than 400 modules says <em>truncated</em> in the header rather than quietly drawing less than it has.
</p>
