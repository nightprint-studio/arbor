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

<h2>The five zones</h2>
<ul>
  <li><b>Title bar</b> — project, and the connection every new tab binds to.</li>
  <li><b>Activity bar</b> — Connections, Scripts on disk, Generate DML, Inventory. The
    consistency indicator sits apart at the bottom: it opens the bottom panel, not a sidebar.</li>
  <li><b>Sidebar</b> — the active section's tree.</li>
  <li><b>Centre</b> — tabs: the generator, query editors, tables, script files, the inventory.</li>
  <li><b>Bottom panel</b> — Consistency, Output, Changes.</li>
  <li><b>Status bar</b> — connection, engine, database version, the open file's encoding,
    the open-findings counter (click it to jump to the report), and the project path.</li>
</ul>

<h2>The rule everything else follows</h2>
<p>
  <b>The dialect belongs to the folder, not to the project.</b> There is no "current
  dialect" anywhere in Picus. A folder declares which engine its scripts are written for,
  everything beneath it inherits that until another folder says otherwise, and every
  operation that reads, analyses or writes SQL is told which engine it is working in. That
  is why the same generation produces four different files, and why each of them is
  correct on its own terms.
</p>
