<script lang="ts">
  /** Picus docs — consistency rules. */
</script>

<h1>Consistency</h1>
<p class="doc-lead">
  The consistency report is the panel Picus is judged on. Every rule has a stable
  identifier, a severity, and a message that states the <b>practical consequence</b> —
  not a restatement of the rule.
</p>

<h2>When it runs</h2>
<p>
  Opening a connection's repository starts the check <b>in the background</b>. The tree is
  usable immediately and the report fills in behind it; nothing waits on it. A re-check
  keeps the previous report on screen with a quiet marker in the panel header rather than
  blanking what you were reading, and only the very first pass of a repository shows a
  loading block.
</p>
<p>
  Checking a real repository takes about a second, so the <b>status bar</b> reads
  <i>Checking…</i> while it runs, and <i>Not checked</i> before the first pass. The panel
  that shows the detail is often closed, and a counter quietly reading "Consistent" during
  an analysis would be a lie with a plausible face.
</p>

<h2>What is compared with what</h2>
<p>
  The cross-engine rules compare <b>one engine's scripts at one role</b> against the other's at the
  same role — the folders that initialise Oracle against the folders that initialise PostgreSQL,
  wherever in the tree they sit and however many of them there are. A repository that splits its
  updates across <code>2024/ORA</code> and <code>2025/ORA</code> still has one update story, so
  reading either half alone would report the other as a gap.
</p>
<p>
  Two kinds of object are left out of all of this. The <b>engine's own</b> —
  <code>user_tab_cols</code>, <code>dual</code>, <code>v$…</code>, <code>pg_…</code>, anything
  under <code>sys</code> or <code>information_schema</code> — are never indexed at all: nobody
  installs them and no counterpart is coming. And an object this repository <b>only reads</b> is
  indexed, marked in the Inventory, and never counted as a gap: a table another repository
  installs and a view here reads is the boundary of the project, not a difference between the
  two engines.
</p>
<p>
  Which engine a script belongs to is the <b>file's</b> answer, not its folder's, so a directory
  holding both <code>4_12_ORA.sql</code> and <code>4_12_POS.sql</code> takes part in both engines'
  comparisons, and each side is credited only with what its own scripts do. A script no folder and
  no name has classified takes part in none of this: comparing an unclassified folder against the
  Oracle ones would report every object in the repository as missing from it.
</p>

<h2>What your initialisation folders are</h2>
<p>
  Two folders can both hold <code>INSERT</code>s and mean completely different things, and no
  amount of reading the SQL settles which. So the project says it, on
  <b>Settings → Analysis</b>, and the choice decides which half of the install-versus-upgrade
  check is even a question:
</p>
<ul>
  <li>
    <b>Cumulative</b> — the initialisation is kept at the latest version. A row it holds that no
    update carries is a first-release row, and there is no update for the beginning, so
    <code>CONS002</code> does not run. <code>CONS003</code> does: a row an update adds must also be
    seeded, or a fresh install comes up missing something every older database has.
  </li>
  <li>
    <b>Mirrored</b> — the two halves are two accounts of the same changes and must agree in both
    directions. Both rules run.
  </li>
  <li>
    <b>Independent</b> — the two halves are maintained separately and comparing them says nothing.
    Neither runs.
  </li>
</ul>
<p>
  The update folder is read as a <b>chain applied in order</b>, not as a set: a row version 1.11
  wrote and version 1.13 replaced is not reported, because it has not existed since 1.13. Only
  the updates' last word about a row is compared with the initialisation.
</p>
<p>
  Cumulative is the default, and the cost of it is worth knowing: adding a row to the
  initialisation and forgetting the matching update script is a real mistake, and nothing readable
  from the tree tells that mistake apart from an ordinary first-release row. Choose
  <b>Mirrored</b> if your initialisation is frozen at the first release.
</p>

<h2>When the two dialects are not comparable</h2>
<p>
  Some repositories have drifted far enough that comparing their two halves says nothing anyone
  can act on — different layouts, different table names, one side generated and the other
  written by hand. <b>Settings → Analysis</b> has a switch for it: with the comparison off,
  <code>CONS001</code> and <code>CONS004</code> stand down and report themselves as not run,
  while the version chain, the duplicates, the dangerous DML and the encodings carry on. It is
  one decision about the repository, so it is one switch rather than two rules to remember.
</p>

<h2>Excusing one object</h2>
<p>
  Every real repository has a handful of tables that are a special case for a reason nothing in
  the scripts can express — a staging table one dialect fills and the other does not need, a log
  the installer writes to, a table kept alive for one customer. List them under
  <b>Settings → Analysis</b> and no rule will say anything about them.
</p>
<p>
  Prefer this to switching a rule off. A rule silenced to quieten one table stops watching the
  other four hundred; a named object costs exactly what it says. The name is matched the way
  every identifier here is matched — case-insensitively when unquoted — and whatever kind of
  object carries it.
</p>
<p>
  An excluded object still appears in the <b>Inventory</b> with its coverage. What is in the
  repository and what should be checked are different questions, and answering the first one
  wrongly would hide the very thing you reasoned about when you excluded it.
</p>

<h2>Switching a rule off</h2>
<p>
  A repository can decide a rule has nothing useful to say about it — views that reference tables
  installed from another repository make <code>CONS001</code> noise rather than signal. Turn it off
  on <b>Settings → Analysis</b>; the decision is written into the project's configuration, so
  everyone working on the repository gets the same report.
</p>
<p>
  A rule that is off is <b>never silently absent</b>. It appears among the rules that could not run,
  naming this setting as the reason, for the same reason everything else here does: a report that
  found nothing has to be distinguishable from a report that did not look.
</p>
<p>
  This is a decision about the whole repository. To excuse <i>one</i> statement, write a suppression
  comment next to it instead — it carries a reason, and the finding stays visible with the reason
  attached.
</p>

<h2>The rules</h2>
<table>
  <thead>
    <tr><th>Id</th><th>Rule</th><th>Severity</th></tr>
  </thead>
  <tbody>
    <tr><td><code>CONS001</code></td><td>An object one engine's scripts <b>change</b> — create, alter, write to, drop, truncate — and the other's never do at the same role. A table merely <i>read</i> in a <code>FROM</code> does not count, so a view over tables another repository installs raises nothing</td><td>blocking</td></tr>
    <tr><td><code>CONS002</code></td><td>Datum in the initialisation, never propagated to the updates</td><td>blocking</td></tr>
    <tr><td><code>CONS003</code></td><td>Datum in an update, missing from the initialisation — a fresh install ends up incomplete</td><td>blocking</td></tr>
    <tr><td><code>CONS004</code></td><td>Object filled in differently for the two engines — same row, different columns or different values</td><td>blocking</td></tr>
    <tr><td><code>DIA001</code></td><td>Statement written in the other dialect's syntax, in a folder that will run against this one</td><td>blocking</td></tr>
    <tr><td><code>VER001</code></td><td>Update block that guards against none of the project's version tables</td><td>blocking</td></tr>
    <tr><td><code>VER002</code></td><td>Block that changes data without carrying a version forward</td><td>blocking</td></tr>
    <tr><td><code>VER003</code></td><td>Version chain with holes or overlaps between update files</td><td>blocking</td></tr>
    <tr><td><code>DUP001</code></td><td>Same row inserted twice in one script, with no <code>DELETE</code> or <code>TRUNCATE</code> of that table between them</td><td>blocking</td></tr>
    <tr><td><code>DUP002</code></td><td>Object created in more than one file that the same half of the install story runs, for one engine and one declared product. <code>CREATE OR REPLACE</code> is exempt — replacing is what it is for</td><td>worth checking</td></tr>
    <tr><td><code>ENC001</code></td><td>File whose encoding changed from the expected one</td><td>worth checking</td></tr>
    <tr><td><code>ENC002</code></td><td>Character not representable in the destination encoding</td><td>blocking</td></tr>
    <tr><td><code>DML001</code></td><td>DELETE or UPDATE with no WHERE, not marked as intentional</td><td>worth checking</td></tr>
    <tr><td><code>DML002</code></td><td>INSERT with no explicit column list</td><td>worth checking</td></tr>
  </tbody>
</table>

<h2>Corrective actions</h2>
<p>
  Where a rule can fix itself, it offers an action. That action <b>proposes a patch</b>
  for review — it is never applied on its own, and it goes through the same diff and the
  same confirmation as any other write.
</p>

<h2>Suppressing a finding</h2>
<p>
  A rule can be silenced on a specific statement with a declared comment in the script:
</p>
<pre><code>-- picus: ignore DML001 — full reload of the parameter table on install</code></pre>
<p>
  The reason is mandatory and stays visible in the report: silencing without a motivation
  is not possible. A suppressed finding is <b>silenced, not deleted</b> — it is hidden by
  default, revealed with the eye toggle in the panel header, and shown dimmed with its
  declared reason spelled out underneath. That is the whole point of requiring a written
  reason: someone reading the repository months later can see what was waved through and
  why.
</p>
<p>
  A suppression comment that names nothing, or names a rule that never fired where it sits,
  is listed under <b>Suppressions that did not apply</b>. Somebody believes that line is
  silencing something, and it is not.
</p>

<h2>Rules that could not run</h2>
<p>
  A rule that could not run is <b>not</b> a rule that passed. <code>VER003</code> standing
  down because the update filenames yield no version bounds means the version chain is
  <i>unchecked</i>, not sound. Every such rule is listed at the foot of the report with its
  scope and its reason, and an otherwise empty report says plainly that finding nothing is
  not the same as nothing being wrong.
</p>

<h2>Taking it elsewhere</h2>
<p>
  A report usually ends up somewhere else — a ticket, a commit message, a conversation with
  whoever wrote the other dialect's half. Every row has a copy button, and the panel header
  copies the whole list <b>as it is filtered</b>, with a heading saying how many findings that
  is and which filter produced them. The text carries the rule id, the severity, the
  <code>path:line</code> and, crucially, the consequence — the sentence about what actually
  goes wrong, which is the part worth pasting.
</p>

<h2>Getting around</h2>
<ul>
  <li><kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>K</kbd> re-runs the rules.</li>
  <li><kbd>F8</kbd> / <kbd>Shift</kbd>+<kbd>F8</kbd> walk the report in reading order, each
    step opening the finding's file at its line and marking the row it landed on.</li>
  <li>The counter in the status bar opens the report; a finding's location opens the file
    at the right place, and for rules that pair two places — a duplicate, an object defined
    twice — the second location is a link of its own.</li>
  <li>Findings group by severity, by folder or by file.</li>
</ul>
