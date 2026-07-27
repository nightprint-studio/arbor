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
  A script repository usually holds the same logical change twice: once in the Oracle
  branch, once in the PostgreSQL one, written in two different syntaxes. Keeping those
  in step by hand produces the same failures over and over — a row added to the
  initialisation and forgotten in the updates, an update block with no starting-version
  guard that re-applies itself, a file saved by another editor that quietly turns
  windows-1252 into UTF-8 and mangles every accented description.
</p>
<p>
  Everything in Picus is aimed at one of those failures. If a feature doesn't reduce
  one, it doesn't belong here.
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
  dialect" anywhere in Picus. Every branch declares its own, and every operation that
  reads, analyses or writes SQL is told which one it is working in. That is why the same
  generation produces four different files, and why each of them is correct on its own
  terms.
</p>
