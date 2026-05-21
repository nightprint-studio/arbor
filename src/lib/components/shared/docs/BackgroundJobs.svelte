<h1>Background Jobs</h1>

<p class="doc-lead">The Jobs system lets plugins run long-running processes in the background — builds, tests, deploys — without blocking the UI. Output is streamed line by line in real time.</p>

<h2>Job lifecycle</h2>
<ol class="step-list">
  <li>Plugin calls <code>arbor.job.spawn(config)</code> — a background thread starts the process immediately</li>
  <li>Each stdout/stderr line fires a Tauri event — the frontend appends it to the job's output buffer in real time</li>
  <li>When the process exits, the <code>on_done_action</code> Lua hook is called and the job status is updated</li>
  <li>If cancelled by the user, the process is killed (<code>SIGTERM</code> on Unix, <code>taskkill /T</code> on Windows)</li>
</ol>

<h2>Status bar badge</h2>
<p>While jobs are running, a badge appears in the status bar (right side). Click it to open the Jobs overlay.</p>
<ul>
  <li><strong>Spinning ● N</strong> (accent colour) — N jobs are currently running</li>
  <li><strong>● N</strong> (green dot) — all done, N total since last clear</li>
</ul>

<h2>Jobs overlay &amp; output panel</h2>
<div class="feature-grid two-col">
  <div class="feature-card">
    <div class="fc-title">Jobs Overlay</div>
    <div class="fc-desc">Floating panel anchored above the status bar. Lists all jobs with status, elapsed time, and plugin name. Each job has a cancel button and an "open output" button (↗).</div>
  </div>
  <div class="feature-card">
    <div class="fc-title">Job Output Panel</div>
    <div class="fc-desc">Read-only terminal-like view docked in the bottom area. Real-time streaming with colour-coded lines (stderr in red, warnings in yellow). Auto-scrolls to latest output; "Jump to latest" pill appears when you scroll up manually.</div>
  </div>
</div>

<h2>Job categories</h2>
<p>Pass a <code>category</code> string to <code>arbor.job.spawn()</code> to group related jobs into collapsible sections in the overlay. Leading/trailing whitespace is trimmed automatically.</p>
<ul>
  <li>Jobs in the same category are shown under a shared collapsible header.</li>
  <li>The header turns accent-coloured and shows a spinner badge when any job in the group is running.</li>
  <li>Running jobs also display a <strong>LIVE</strong> badge next to their name.</li>
  <li>Jobs without a category are listed below all named groups.</li>
</ul>
<p>Recommended conventions: <code>"Builds"</code> for compilation tasks, <code>"Services"</code> for long-running processes (dev servers, application runners).</p>

<h2>Non-cancellable jobs</h2>
<p>
  Some jobs are marked as <strong>non-cancellable</strong> — they are system tasks that Arbor
  manages internally and that must not be interrupted by the user.
  For these jobs the cancel / stop button is hidden in both the Jobs overlay and the output panel.
</p>
<ul>
  <li>They still appear in the overlay and output panel like any other job, with a real-time output stream.</li>
  <li>They can finish naturally (<em>Completed</em>) or fail (<em>Failed</em>); they are never <em>Cancelled</em>.</li>
  <li>Plugin <code>reload_plugins</code> skips non-cancellable jobs — they are not affected by plugin reloads.</li>
</ul>

<div class="callout warning">
  <strong>Reserved category: <code>"system"</code></strong>
  The category <code>"system"</code> (case-insensitive) is reserved for Arbor's own internal background jobs.
  Calling <code>arbor.job.spawn()</code> with this category from a plugin raises a Lua error.
  System jobs are also <strong>automatically dismissed</strong> from the overlay once they complete successfully —
  they are designed to run silently and leave no trace on a clean exit.
</div>

<h2>Hidden jobs</h2>
<p>
  Jobs spawned with <code>hidden = true</code> are excluded from the default Jobs overlay and Job Output panel listings,
  and from the status-bar running-job badge. They are intended for jobs owned by a domain-specific panel (for example a
  Services panel that manages long-running app servers like Tomcat) where the host Jobs UI would be redundant.
</p>
<ul>
  <li>The job still runs, streams output, and fires <code>on_done</code> hooks normally.</li>
  <li>A <strong>Show hidden</strong> toggle in the Jobs overlay and Job Output panel headers reveals them when needed (for example, to kill a zombie service). The toggle state is shared between both panels and persisted in <code>localStorage</code>.</li>
  <li>When the toggle is on, hidden jobs are also counted by the status-bar badge.</li>
  <li>If only hidden jobs exist, the overlay shows a hint instead of the empty state.</li>
</ul>

<h2>Output ring buffer</h2>
<p>Each job stores the last <strong>2 000 lines</strong> of output in memory (oldest lines dropped when exceeded) and on disk — so you can view output after reopening the overlay or restarting the app.</p>

<div class="callout info">
  <strong>Background jobs vs. terminal</strong>
  Background jobs are designed for automated tasks triggered by plugins. For interactive work (running a dev server, using a REPL) use the built-in <strong>Terminal</strong> instead.
</div>

<h2>Job sequencing</h2>
<p>Jobs can be chained by attaching <code>:ok</code> / <code>:err</code> on the returned <code>JobHandle</code>, by passing an <code>on_done</code> sugar callback, or by awaiting inside <code>arbor.async.run</code>. Common patterns:</p>
<ul>
  <li><strong>Build → run</strong>: spawn the build, then chain <code>build:ok(function(_) spawn_service() end)</code>.</li>
  <li><strong>Queue</strong>: if a build is already running, record the pending run in plugin state; the build's <code>:ok</code> starts it when done.</li>
  <li><strong>Mutual exclusion</strong>: track <code>active_build_id</code> in state — reject or queue conflicting jobs.</li>
</ul>
<p>The compile-action plugin uses all three patterns: pressing <strong>F5</strong> while a build is running queues the run automatically; pressing <strong>F9</strong> while a service is running stops the service first, then builds.</p>

<h2>Plugin API</h2>
<table class="shortcuts-table">
  <thead><tr><th>Function</th><th>Description</th></tr></thead>
  <tbody>
    <tr><td><code>arbor.job.spawn(config)</code></td><td>Launch a background job. Returns <code>(JobHandle, nil)</code> on success or <code>(nil, err)</code> on a spawn-side failure. The handle is a Promise (<code>:ok / :err</code>) with extra <code>.id</code> and <code>:cancel()</code>. Config: <code>name</code>, <code>command</code>, <code>cwd?</code>, <code>env?</code>, <code>category?</code> (string), <code>hidden?</code> (boolean — hide from Jobs panels and badge by default), <code>on_done?</code> (callback — sugar), <code>on_done_action?</code> (hook name — sugar)</td></tr>
    <tr><td><code>arbor.job.cancel(job_id)</code></td><td>Kill a running job (SIGTERM / taskkill /T). No-op if already finished. Useful to stop long-running processes (servers, watchers) before re-launching them.</td></tr>
    <tr><td><code>arbor.job.list()</code></td><td>Returns a Lua table of all job records with fields: <code>id</code>, <code>name</code>, <code>status</code>, <code>started_at</code></td></tr>
  </tbody>
</table>
<p>See the <strong>Plugin Development</strong> section for full examples.</p>
