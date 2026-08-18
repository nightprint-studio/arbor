<!-- Bennu docs — the module graph window. -->
<h1>The module graph</h1>
<p class="doc-lead">
  The Dependencies panel answers <em>what does this module need</em>. Four questions it cannot answer
  are properties of the project's <strong>shape</strong> rather than of any one row, and in a workspace
  of twenty crates they are the ones you arrive with. <kbd>Alt</kbd> + <kbd>Shift</kbd> + <kbd>D</kbd>,
  or the network button on the Dependencies and Cargo windows, draws them.
</p>
<ul>
  <li><strong>Who uses this</strong> — and therefore whether anything still does.</li>
  <li><strong>What breaks if I touch it</strong> — how many modules rebuild, transitively.</li>
  <li><strong>What is foundational</strong> — what sits at the bottom and is worth being careful with.</li>
  <li><strong>Is there a cycle</strong> — which cargo refuses to build and Maven refuses to order.</li>
</ul>
<p>
  It works for a Maven reactor as well as a Cargo workspace, from the same reading of the manifests —
  nothing runs cargo or Maven, so it opens on a project that has never been built. The window says
  <em>crates</em> or <em>modules</em> depending on which it is looking at.
</p>

<h2>Reading it</h2>
<p>
  Layers run <strong>left to right: dependents first, the foundation last</strong>, so a chain reads
  like the sentence describing it — <code>app → core → util</code>. An arrow points from the module
  that <em>declares</em> the dependency to the one it depends on, which means every arrow in a healthy
  project points rightwards.
</p>
<p>
  That is the reason the layout is that way round rather than the other: <strong>a leftward arrow is a
  cycle</strong>. It is visible as a shape, without counting anything or consulting a colour.
</p>
<p>Each column is labelled with its <strong>layer</strong> — how far above the foundation it sits.
  Layer 0 depends on nothing else in the project; a module is one layer above the deepest thing it
  depends on. Every module of a cycle shares a layer, because there is no order between them.</p>

<h2>What the lines mean</h2>
<p>
  The <strong>legend</strong> button in the window's footer draws each mark beside its meaning; it is
  open the first time you arrive and stays closed once you have closed it. In words:
</p>
<table>
  <thead><tr><th>Line</th><th>Means</th></tr></thead>
  <tbody>
    <tr>
      <td><strong>solid</strong></td>
      <td>An ordinary dependency — a Cargo <code>dependencies</code> or <code>build-dependencies</code>
        entry, a Maven <code>compile</code> / <code>provided</code> / <code>runtime</code> scope.
        <strong>It orders the build.</strong></td>
    </tr>
    <tr>
      <td><strong>dashed</strong></td>
      <td>A Cargo <code>dev-dependency</code>, or a Maven <code>test</code> scope. Real, and it does
        <em>not</em> order the build. Under cargo it may <strong>legally close a cycle</strong> — a
        crate's tests are compiled as a separate unit, so a library whose tests use something that
        depends on it is a normal arrangement, not an error. Maven refuses a cycle through one anyway,
        because it orders the whole reactor as a single graph. Either way it counts towards
        <em>what rebuilds</em>: changing the library does rebuild the tests that use it.</td>
    </tr>
    <tr>
      <td><strong>dotted</strong></td>
      <td>Optional — on the graph only when a feature turns it on. Whether that has happened is not a
        fact about the manifest, so it is drawn and labelled rather than guessed at.</td>
    </tr>
    <tr>
      <td><strong>red</strong></td>
      <td>Part of a <strong>cycle</strong>: the build tool refuses this.</td>
    </tr>
    <tr>
      <td><strong>blue</strong></td>
      <td>Touching the module you have selected — or the one under the pointer. Resting on a box lights
        its own lines, which is the quickest way to answer "which of these is mine".</td>
    </tr>
  </tbody>
</table>
<p>
  So the distinction the two commonest styles draw is a single one: <strong>solid dependencies decide
  the order things are built in; dashed ones do not.</strong> That is exactly why the cycle check
  ignores the dashed ones under cargo and not under Maven.
</p>
<p>
  On the boxes: the <strong>bar</strong> on the left is what the module builds — a library, a program,
  both, a proc-macro; a jar, a war, or an aggregator pom that builds nothing. The <strong>number</strong>
  on the right is how many modules rebuild when it changes. A <strong>red border</strong> means it is in
  a cycle, a blue one that it is selected.
</p>
<p>
  An edge that crosses several layers is <strong>routed between the boxes</strong> in the columns it
  passes rather than straight across them. That is what the extra vertical room buys — a long
  dependency you can follow instead of a line disappearing under three other modules.
</p>

<h2>The list, and the numbers</h2>
<p>
  Beside the drawing is every module as a row, and it is not a lesser view: finding the crate whose
  name you half remember in a picture of forty boxes is a scan, and here it is three keystrokes. It is
  also the <strong>keyboard surface</strong> — <kbd>↑</kbd> <kbd>↓</kbd> walk it, <kbd>Enter</kbd> opens
  the manifest — so the whole window works without touching the graph.
</p>
<p>Sort it by:</p>
<ul>
  <li><strong>Most rebuilt on</strong> — how many modules a change here reaches. The number to know
    before touching something, and the reason a leaf with a high one is not really a leaf.</li>
  <li><strong>Layer</strong> — deepest first, matching the drawing's order.</li>
  <li><strong>Most third-party</strong> — which module pulls the most outside code in.</li>
  <li><strong>Name</strong>.</li>
</ul>
<p>
  Selecting is one act across all three views: the list moves the drawing, the drawing moves the list,
  and either fills the detail panel underneath — the module's numbers, and its direct dependencies and
  dependents as rows you can walk into.
</p>
<p>
  <strong>Used by: nothing</strong> is stated as a fact and not as a verdict. A library published to a
  registry and a deployable war both legitimately have nothing inside the project depending on them; in
  a private workspace a library in that state is usually dead code. Only you know which.
</p>

<h2>Solo, and searching</h2>
<p>
  <strong>Solo</strong> (<kbd>Alt</kbd> + <kbd>S</kbd>) draws only the selected module's world and drops
  everything else. It is a <em>filter</em>, not a dimming: the columns are recomputed from what is left
  and the empty ones collapse, so in a workspace of sixty crates the other fifty stop taking up room.
  Three scopes — everything connected, only what it is <strong>built on</strong>, or only what it would
  <strong>break</strong>.
</p>
<p>
  Solo follows the selection, so picking another module re-isolates around that one: it is how you walk
  a dependency chain a crate at a time without leaving the mode. The header counts what is on screen
  (<em>4 of 22 crates</em>) rather than what exists, because quoting the project's totals would describe
  a picture you are not looking at.
</p>
<p>
  <strong>Searching</strong> is the other way to narrow, and it dims rather than filters: the matches
  stay lit, everything else recedes without moving. The two answer different questions, and both can be
  on — the search then dims inside the soloed world.
</p>

<h2>Cycles</h2>
<p>
  A cycle is counted in the header, and pressing the count goes to one. The whole <strong>ring</strong>
  is named rather than the single pair the build tool mentions when it refuses: five crates that all
  reach each other usually contain several rings, and printing one would suggest the others are fine.
</p>
<p>
  What counts as a cycle is the build tool's own answer, not a guess — see the dashed row above. And the
  drawing agrees with the count by construction: the members share a layer, so their arrows are the ones
  pointing the wrong way.
</p>

<h2>Taking it elsewhere</h2>
<p>
  The <strong>export</strong> button beside the window's ✕ copies the graph or writes it to a file.
  There is no import: the manifests are the truth, and this is a description of them.
</p>
<ul>
  <li><strong>Markdown</strong> — for a language model, or a person. Each module with what it depends on
    and what depends on it, then the three lists worth having answered: most expensive to change, the
    ones nothing depends on, and the cycles. It <em>says</em> what each number means, which costs fewer
    tokens than a schema the reader has to infer.</li>
  <li><strong>JSON</strong> — for a script. Every field, exactly as computed, keyed by module name
    rather than by array index.</li>
  <li><strong>CSV</strong> — for a spreadsheet. One row per edge, with a column saying whether it
    orders the build.</li>
</ul>
<p>
  It exports <strong>what is on screen</strong> — solo and the dashed-edge filter included — and the
  file's own header says which filters were on. An export that silently described the whole project
  while the window showed one crate's neighbourhood would mislead whoever, or whatever, read it.
</p>
<p>
  RON is deliberately not offered: it would be a second spelling of the JSON with nothing that prefers
  to read it.
</p>

<h2>What it will not tell you</h2>
<p>
  Everything here is read from the manifests, which is what makes it instant and makes it work on a
  project that has never compiled. The cost is stated rather than hidden: <strong>nothing is
  resolved</strong>. Feature unification across the workspace, which dependencies a
  <code>cfg(…)</code> actually admits, and whether a Maven profile is active are cargo's and Maven's
  answers, not the manifests'. Conditional and optional edges are therefore drawn and labelled.
</p>
<p>
  A project with more than 400 modules says <em>truncated</em> in the header rather than quietly drawing
  less than it has.
</p>
