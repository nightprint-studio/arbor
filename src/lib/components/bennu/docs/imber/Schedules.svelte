<span class="eyebrow">Imber</span>
<h1>Imber: Schedules</h1>

<p class="doc-lead">
  A schedule asks Imber something at a time you choose: once, every N minutes, every day, on chosen weekdays, or on a cron expression. "Every weekday at 09:00, tell me what changed on main" is one schedule.
</p>

<h2>Creating one</h2>
<ul>
  <li><strong>By hand.</strong> Open the window from <strong>Schedules…</strong> in the panel's <strong>…</strong> menu or the palette entry <em>Imber: Schedules…</em>, then <strong>New</strong> (or press <kbd>N</kbd> in the list).</li>
  <li><strong>From the composer.</strong> Type the prompt, then run <em>Imber: Schedule this prompt…</em> from the palette: the form opens with the text already in it. The entry appears only when the composer has text and a project is open.</li>
  <li><strong>By asking Imber.</strong> Say it in a conversation: "every weekday at 9, check what changed on main". Imber always shows the schedule in words and waits for your approval before it exists, whatever the access mode.</li>
</ul>
<p>
  The form is short: name, when, what it does, what to do when the run was missed, and an on/off switch. The name takes the focus when it opens, <kbd>Tab</kbd> goes through the fields, <kbd>Ctrl</kbd>+<kbd>Enter</kbd> saves and <kbd>Esc</kbd> goes back.
</p>

<h2>What a schedule does</h2>
<table>
  <thead><tr><th>Action</th><th>What happens</th></tr></thead>
  <tbody>
    <tr><td><strong>Wake this conversation</strong></td><td>The saved prompt is sent into the conversation the schedule was made from, which carries on as if you had written it.</td></tr>
    <tr><td><strong>Ask Imber in a new conversation</strong></td><td>A fresh conversation each time, titled after the schedule. The outcome is shown on the schedule.</td></tr>
  </tbody>
</table>
<p>
  Write the prompt so that it needs no answer: say what to check and what to report. Nobody is there to reply.
</p>

<h2>A scheduled run is read-only</h2>
<ul>
  <li>The run reads and searches the project, and any change it wants to make is left as a proposal in its conversation, waiting for you. It never modifies the project, runs commands or asks for confirmation.</li>
  <li>If Imber asks a question, the run ends as <em>incomplete</em> instead of waiting.</li>
  <li>A scheduled run cannot create, change or delete schedules.</li>
  <li>A schedule Imber creates is no freer than the conversation that created it, and an MCP client can only create read-only ones.</li>
</ul>

<h2>Only while Bennu is open</h2>
<p>
  Schedules fire only while Bennu is running; there is no background service. When a run's time passed while Bennu was closed, the schedule decides: <strong>Skip it</strong> (the default) drops it, and the schedule shows it as <em>Skipped</em> rather than as a failure; <strong>Run it once on the next start</strong> catches up with a single run. A one-time schedule that was missed and skipped turns itself off.
</p>

<h2>Cost limits</h2>
<p>
  Scheduled runs spend from the same limits as the panel and are also capped on their own: a <strong>scheduled run cap</strong> (euros per run; the run ends as incomplete when it gets there) and a number of <strong>scheduled runs per day</strong>, so a mistaken schedule cannot spend the month overnight. Both are in Settings › Imber › Spending limits; 0 switches one off.
</p>

<h2>Knowing it ran</h2>
<p>
  A small toast says "Schedule … started" at every start, including <strong>Run now</strong>, and a failure shows its reason, once per episode rather than at every attempt. The list shows when each schedule runs next and how the last run ended.
</p>

<h2>The window</h2>
<table>
  <thead><tr><th>Key</th><th>Action</th></tr></thead>
  <tbody>
    <tr><td><kbd>↑</kbd> <kbd>↓</kbd></td><td>Move between schedules</td></tr>
    <tr><td><kbd>Enter</kbd></td><td>Edit the selected schedule</td></tr>
    <tr><td><kbd>Space</kbd></td><td>Turn it on or off</td></tr>
    <tr><td><kbd>R</kbd></td><td>Run it now, outside its times (its next run is unchanged)</td></tr>
    <tr><td><kbd>Del</kbd></td><td>Delete it, after a confirmation</td></tr>
    <tr><td><kbd>N</kbd></td><td>New schedule</td></tr>
  </tbody>
</table>
<p>A schedule that wakes a conversation also has a button that opens it.</p>

<h2>From other tools</h2>
<p>
  AI clients connected over MCP get two tools: <code>bennu_imber_schedules</code> lists the schedules, and <code>bennu_imber_schedule_create</code> creates a read-only one. Neither can change or delete an existing schedule.
</p>

<h2>Where they are saved</h2>
<p>
  The list lives in Bennu's config folder, outside your projects, so a run's bookkeeping never shows up in version control. Each schedule belongs to the project that was open when it was made.
</p>
