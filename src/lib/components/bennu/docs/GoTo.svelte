<!-- Bennu docs — go-to-anything: reaching a class, a file or a symbol by name. -->
<h1>Go to class, file, symbol</h1>
<p class="doc-lead">
  Reaching something by its name, including the names that are not in your source tree at all.
</p>

<h2>Go to class / file / symbol</h2>
<p>
  One navigator over three lists, with a tab each: <strong>Classes</strong>, <strong>Files</strong>
  and <strong>Symbols</strong> (every method and field the project declares), plus an
  <strong>All</strong> tab that searches them together and keeps each one's best few under its own
  heading. <kbd>Ctrl</kbd> + <kbd>N</kbd>, <kbd>Ctrl</kbd> + <kbd>Shift</kbd> + <kbd>N</kbd> and
  <kbd>Ctrl</kbd> + <kbd>Shift</kbd> + <kbd>Y</kbd> open it on Classes, Files and Symbols
  respectively; <kbd>Tab</kbd> moves between them without reopening. The tabs sit above the field
  because they decide what it searches. The field has the caret the moment it opens, and
  <strong>nothing is read until you type</strong>: listing every class in a reactor is a wait
  paid on every use for an answer nobody scrolls.
</p>
<p>
  The selected entry is shown <strong>in context</strong> on the right — the declaration with the
  lines around it, or a file’s head — syntax-coloured. A list of names says where there is an
  <code>OrderDao</code>; it does not say whether it is <em>the</em> one you meant, and on a legacy
  tree with four classes of that name that is the only question left. Walking with ↑/↓ re-reads as
  you go; nothing is opened until <kbd>Enter</kbd>.
</p>
<p>
  On a multi-module project a <strong>module</strong> dropdown appears on the header row of the
  Classes and Files tabs. It lists only the modules that actually have something in them, and a
  nested module wins over the parent that lists it — a class under <code>modules/core</code> is
  filed there, not under <code>modules</code>. Switching tab clears it, since a module that exists
  for classes need not exist for files. Every row also carries where it came from: the
  <strong>module</strong> for something this build compiles, the <strong>project</strong> for a
  sibling of it, the <strong>artifact</strong> for something it only depends on.
</p>
<p>
  On a workspace, the toggle beside the field — the same one Find in project has — makes Classes
  and Files read <strong>every member project</strong>, not just the active one. A member's class
  index is built the first time you search it and reused after that.
</p>
<p>
  Matching is by <strong>subsequence</strong>, not substring: <code>agpo</code> finds
  <code>AGGIORNAMENTO/POS</code> because the letters appear in order, and the characters that
  matched are lit in the row so a loose match is legible rather than mysterious. Several terms are
  each matched independently, so <code>agg pos</code> works on a path that separates them. Results
  rank on where the hit lands — the start of a word beats the middle, a run beats a scattered match,
  a short name beats a long one.
</p>
<p>
  Three directives narrow a search further: <code>in:dao</code> (the path contains it),
  <code>ext:java</code> (that extension) and <code>sort:new</code> (most recently modified first).
  ↑/↓ move, <kbd>Enter</kbd> opens — a class or symbol jumps straight to its declaration line. A word
  selected in the editor pre-fills the field. Files work in any project; Classes and Symbols read the
  Java index, so they aren't offered in a Cargo one.
</p>

<h3>Reaching what is inside the dependencies</h3>
<p>
  The <strong>Source</strong> picker on the header row decides whose code the Classes and Files
  tabs are about: <strong>Project</strong>, <strong>Dependencies</strong>, or
  <strong>Project &amp; dependencies</strong> — which ranks the two into one list rather than
  making you check two tabs for one question. That is how you reach what is on the classpath and
  nowhere in the tree: the framework annotation whose package you are trying to remember, the
  <code>struts-default.xml</code> that declares the interceptor stack, the schema an XML file is
  validated against.
</p>
<p>
  A row from a dependency is <strong>tinted</strong> and says which <strong>artifact</strong> it
  came from — a classpath is where four versions of the same name live, and what you can read is
  not what you can change. The same picker is on <strong>Find in project</strong>, where
  <em>Dependencies</em> alone is often what you want: the schema or the interceptor stack some
  artifact declares, with every hit in your own tree being noise.
</p>
<p>
  <strong>Search the dependencies too</strong> (Settings → Java) moves the <em>default</em> to
  <em>Project &amp; dependencies</em>. It no longer decides whether the classpath is reachable —
  that is one pick away, in front of you.
</p>
<p>
  Opening a library <em>class</em> shows its source the same way a stack-trace frame does: the real
  <code>.java</code> when the JDK ships sources or a <code>-sources.jar</code> has been downloaded,
  otherwise the decompiled stub. A library <em>file</em> is extracted from the jar and opened
  read-only, keeping its extension — so an XML still reads as XML.
</p>
<p>
  Classpath rows are searched as you type rather than listed: it is hundreds of thousands of
  entries, and nothing is fetched until there is a query. The first search after opening a project
  spends a moment reading the jars, and is instant after that.
</p>
