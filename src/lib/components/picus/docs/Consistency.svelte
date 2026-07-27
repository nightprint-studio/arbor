<script lang="ts">
  /** Picus docs — consistency rules. */
</script>

<h1>Consistency</h1>
<p class="doc-lead">
  The consistency report is the panel Picus is judged on. Every rule has a stable
  identifier, a severity, and a message that states the <b>practical consequence</b> —
  not a restatement of the rule.
</p>

<h2>The rules</h2>
<table>
  <thead>
    <tr><th>Id</th><th>Rule</th><th>Severity</th></tr>
  </thead>
  <tbody>
    <tr><td><code>CONS001</code></td><td>Statement present in one branch and absent from the other dialect's equivalent</td><td>blocking</td></tr>
    <tr><td><code>CONS002</code></td><td>Datum in the initialisation, never propagated to the updates</td><td>blocking</td></tr>
    <tr><td><code>CONS003</code></td><td>Datum in an update, missing from the initialisation — a fresh install ends up incomplete</td><td>blocking</td></tr>
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
  is not possible. Suppressed findings are hidden by default and revealed with the eye
  toggle in the panel header.
</p>

<h2>Getting around</h2>
<ul>
  <li><kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>K</kbd> re-runs the rules.</li>
  <li><kbd>F8</kbd> / <kbd>Shift</kbd>+<kbd>F8</kbd> step through findings.</li>
  <li>The counter in the status bar opens the report; a finding's location opens the file
    at the right place.</li>
  <li>Findings group by severity, by branch or by file.</li>
</ul>
