<script lang="ts">
  /** Picus docs — Getting Started. */
</script>

<h1>Picus</h1>
<p class="doc-lead">
  Picus is Arbor's tool for databases and the SQL scripts that build them. It has two
  halves that live in one window: a <b>database client</b> for Oracle and PostgreSQL,
  and a <b>maintainer of the script repository</b> those databases are installed from.
</p>

<h2>The problem it exists for</h2>
<p>
  A script repository usually holds the same logical change twice: once for Oracle, once
  for PostgreSQL, written in two different syntaxes and often in two folders that look
  nothing alike. Keeping those in step by hand produces the same failures over and over — a row added to the
  initialisation and forgotten in the updates, an update block with no starting-version
  guard that re-applies itself, a file saved by another editor that quietly turns
  windows-1252 into UTF-8 and mangles every accented description.
</p>
<p>
  Everything in Picus is aimed at one of those failures. If a feature doesn't reduce
  one, it doesn't belong here.
</p>

<h2>Where to start</h2>
<p>
  Picus is <b>database-oriented</b>: you open a database, and its scripts are what you see.
  So the first step is a connection — and a connection can carry the folder of SQL scripts
  that database is installed from. Attach one from the connection editor, from the Scripts
  panel, or from the command palette, and opening that connection brings its repository,
  its inventory and its consistency report into the window. A connection you only run
  queries against needs none.
</p>
<p>
  Reading the folder is immediate; checking it is not, so the two are separate. The tree
  appears as soon as the folder is read, and the consistency report fills in behind it
  without ever holding the window. <kbd>F5</kbd> re-reads the folder from disk.
</p>

<h2>The zones</h2>
<ul>
  <li><b>Title bar</b> — project, and the connection every new tab binds to.</li>
  <li><b>Activity bar</b> — Connections, Scripts on disk, Generate DML, Inventory at the top.
    Below them, apart, the buttons that open a <i>bottom</i> panel rather than a sidebar:
    <b>Output</b>, <b>Changes</b> and the consistency indicator.</li>
  <li><b>Sidebar</b> — the active section's tree.</li>
  <li><b>Right rail</b> — the tools that describe the <i>document</i> rather than the project:
    the <b>Syntax tree</b> (<kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>Y</kbd>), the structural
    replace, and <b>Results</b> — the rows the statement in this tab returned.</li>
  <li><b>Centre</b> — tabs: the generator, query editors, tables, script files, the inventory.</li>
  <li><b>Bottom panel</b> — <b>one</b> panel at a time, the one whose button you pressed.
    Every answer the window produces arrives down here, the rows a query returned included;
    running a statement opens Results, and it closes like any other panel.</li>
  <li><b>Status bar</b> — the connection, the open file's encoding, the open-findings counter
    (click it to jump to the report), and the project path.</li>
</ul>

<h2>Finding things</h2>
<p>
  Two boxes, and they answer different questions. <kbd>Ctrl</kbd>+<kbd>K</kbd> is the
  <b>command palette</b>: what can I do. <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>O</kbd> is
  <b>Go to</b>: where is it — every script, every indexed object, every connection, in one
  list. They are separate because merging them produces a ranking where a verb and a
  filename compete, and neither question gets a good answer.
</p>
<p>
  Go to matches loosely and in order, so <code>agg pos</code> finds
  <code>AGGIORNAMENTO/2024/POS/4_13.sql</code> — every term has to land somewhere, in the
  name or in the path, and the matched letters are lit in the row. Three directives narrow
  it further, typed inline anywhere in the query:
</p>
<ul>
  <li><code>sort:new</code> — most recently changed first. Also <code>sort:name</code>,
    <code>sort:name-desc</code>, <code>sort:path</code>.</li>
  <li><code>ext:sql</code> — only that extension.</li>
  <li><code>in:AGGIORNAMENTO</code> — only under a path containing that.</li>
</ul>
<p>
  <kbd>Tab</kbd> cycles the categories, the arrows move, <kbd>Enter</kbd> opens. Anything
  not recognised as a directive stays part of the search text, so a colon in a name never
  makes the box refuse to find it.
</p>

<p>
  This documentation can leave the window: the button beside the title exports it as a
  Markdown README or as a self-contained styled HTML page, every topic in nav order, with
  a table of contents. Useful when the answer belongs in a ticket, a wiki or the
  repository rather than in a panel.
</p>

<h2>The rule everything else follows</h2>
<p>
  <b>The dialect belongs to the script, not to the project.</b> There is no "current
  dialect" anywhere in Picus. A folder declares which engine its scripts are written for,
  everything beneath it inherits that until another folder says otherwise, and every
  operation that reads, analyses or writes SQL is told which engine it is working in. That
  is why the same generation produces four different files, and why each of them is
  correct on its own terms.
</p>
<p>
  The folder is where that answer normally lives, and where it normally should. But a repository
  that keeps <code>4_12_ORA.sql</code> next to <code>4_12_POS.sql</code> in one directory is
  describing its engines file by file, so a <b>file</b> can carry the answer too and overrules the
  folder around it. See <i>Scripts on disk</i>.
</p>
