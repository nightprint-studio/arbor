<script lang="ts">
  /** Hirundo docs — tests and the runner. */
  const ASSERTIONS = `status == 200
time < 500ms
header.content-type contains json
body.content[*].stato == "APERTO"
len(body.content) > 0
body.id exists`;

  const LUA = `--- lua
local r = hirundo.response.json()
hirundo.expect(#r.content > 0, "at least one item")
hirundo.env.set("itemId", r.content[1].id)`;
</script>

<h1>Tests and the runner</h1>
<p class="doc-lead">
  Most tests are one line each. For the rest there is Lua, in a sandbox.
</p>

<h2>Assertions</h2>
<p>One per line, in the request's Tests tab: <code>subject operator value</code>. Lines starting with <code>--</code> are comments.</p>
<pre>{ASSERTIONS}</pre>
<p>
  Subjects: <code>status</code>, <code>time</code>, <code>size</code>, <code>header.name</code>, <code>body</code>, <code>body.path</code> (<code>a.b[0].c</code>,
  <code>[*]</code> = every element must satisfy it), <code>len(…)</code>. Operators: <code>== != &lt; &lt;= &gt; &gt;= contains startswith endswith</code>,
  and <code>exists</code>, <code>not exists</code>, <code>empty</code>.
</p>

<h2>Lua</h2>
<p>A line <code>--- lua</code> starts a script, the same language as the Arbor plugins but in a smaller world: no files, no processes, no <code>require</code>, and a limit on how long it runs.</p>
<pre>{LUA}</pre>
<ul>
  <li><code>hirundo.response</code> — <code>status</code>, <code>time</code>, <code>size</code>, <code>headers</code>, <code>text()</code>, <code>json()</code> (<code>null</code> is <code>nil</code>).</li>
  <li><code>hirundo.expect(condition, message)</code> — a check that shows in the results.</li>
  <li><code>hirundo.env.set(name, value)</code> — a local variable for the next request.</li>
  <li><code>hirundo.log(text)</code>.</li>
</ul>

<h2>The runner</h2>
<p>
  The Runner section sends every request of a collection or folder, in tree order, against the active environment, runs each one's tests
  and lists which passed. A request without tests passes when the server did not answer with an error. From the tree, <i>Run…</i> on a
  collection or folder does the same. Every run is kept in the project's History (in your profile, not in the project's files).
</p>

<h2>Schedules</h2>
<p>
  The Schedules tab runs a collection on a timetable — once, every N minutes, daily, on chosen weekdays or by a cron expression: “every night at
  02:00, Smoke, on staging”. It is the same calendar the Imber schedules use. A schedule fires <b>while the Hirundo window is open</b>; a run that was
  missed meanwhile is skipped, or caught up once if the schedule says so. A run nobody is watching cannot confirm a protected environment, so
  the requests that would have asked are reported as not sent.
</p>
