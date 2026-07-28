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
  Which engine a script belongs to is the <b>file's</b> answer, not its folder's, so a directory
  holding both <code>4_12_ORA.sql</code> and <code>4_12_POS.sql</code> takes part in both engines'
  comparisons, and each side is credited only with what its own scripts do. A script no folder and
  no name has classified takes part in none of this: comparing an unclassified folder against the
  Oracle ones would report every object in the repository as missing from it.
</p>

<h2>The rules</h2>
<table>
  <thead>
    <tr><th>Id</th><th>Rule</th><th>Severity</th></tr>
  </thead>
  <tbody>
    <tr><td><code>CONS001</code></td><td>Statement present for one engine and absent from the other engine's scripts at the same role</td><td>blocking</td></tr>
    <tr><td><code>CONS002</code></td><td>Datum in the initialisation, never propagated to the updates</td><td>blocking</td></tr>
    <tr><td><code>CONS003</code></td><td>Datum in an update, missing from the initialisation — a fresh install ends up incomplete</td><td>blocking</td></tr>
    <tr><td><code>CONS004</code></td><td>Object filled in differently for the two engines — same row, different columns or different values</td><td>blocking</td></tr>
    <tr><td><code>DIA001</code></td><td>Statement written in the other dialect's syntax, in a folder that will run against this one</td><td>blocking</td></tr>
    <tr><td><code>VER001</code></td><td>Update block with no starting-version guard</td><td>blocking</td></tr>
    <tr><td><code>VER002</code></td><td>Block that changes data without carrying the version forward</td><td>blocking</td></tr>
    <tr><td><code>VER003</code></td><td>Version chain with holes or overlaps between update files</td><td>blocking</td></tr>
    <tr><td><code>DUP001</code></td><td>Same key inserted twice in one script</td><td>blocking</td></tr>
    <tr><td><code>DUP002</code></td><td>Object (package, procedure) defined in more than one file</td><td>worth checking</td></tr>
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
