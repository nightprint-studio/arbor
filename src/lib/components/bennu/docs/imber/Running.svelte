<span class="eyebrow">Imber</span>
<h1>Imber: Running and observing</h1>

<p class="doc-lead">
  With the Live services family on, Imber can start the project's services, wait for them, read their logs economically, call them over localhost and run scenarios; builds from every conversation share one queue.
</p>

<p>
  The feature itself (processes, recipes, scenarios, the timeline, correlation ids) is described on the <strong>Live services</strong> page. This page is about how Imber uses it.
</p>

<h2>Managed processes</h2>
<ul>
  <li><code>run_start</code> starts a named process with a command, a working folder, optional ports and a readiness check (a port, a log line or a health URL). It asks every time, showing the command.</li>
  <li><code>run_status</code>, <code>run_logs</code>, <code>wait_ready</code> and <code>run_stop</code> inspect, wait for and stop what the conversation started. Reading never asks; stopping what the run started does not ask.</li>
  <li>The processes are the same ones the Live panel lists. Every process records its owner; a conversation stops only its own processes. A name or port that another conversation holds is refused instead of duplicated or killed, and a stack another conversation already runs is attached read-only.</li>
  <li>A yes given to a recipes file in one conversation does not carry to another.</li>
</ul>

<h2>Recipes and scenarios</h2>
<p>
  <code>list_recipes</code> and <code>run_recipe</code> use the project's recipes file (<code>.arbor/bennu/live/recipes.toml</code>); <code>list_scenarios</code>, <code>run_scenario</code> and <code>save_scenario</code> use scenario files. Running a recipe or a scenario asks once per <em>content</em> of its file: change the file and it asks again. A scenario is an ordered list of steps with assertions and also runs without Imber from the Live panel.
</p>

<h2>Requests and the timeline</h2>
<p>
  <code>http_request</code> reaches services on this machine and nothing else (<code>localhost</code>, <code>127.0.0.1</code>, <code>[::1]</code>). GET, HEAD and OPTIONS do not ask; any other method asks unless the port belongs to a process the conversation started. <code>trace_request</code> follows one request through the logs of every process by its correlation id, and <code>timeline</code> reads the unified timeline of requests, log events and process events.
</p>

<h2>Reading logs without reading them</h2>
<p>A log is read as <em>records</em> (a line with its stack trace, the <code>Caused by</code> chain or a backtrace), digest first, detail only when needed:</p>
<ul>
  <li><strong><code>log_facets</code></strong>: a compact digest instead of lines, for a process, the Run console or a log file: exceptions grouped by class and root cause, queries with literals removed, requests, warnings.</li>
  <li><strong><code>run_logs</code></strong>: a process's records with filters (level, pattern, time range, context) and a summary of the distinct errors.</li>
  <li><strong><code>log_record</code></strong>: one record in full, by the id a digest printed.</li>
  <li><strong><code>wait_log</code></strong>: waits for a new line matching a regular expression (default 30 s, at most 300), with context on each side.</li>
  <li><strong><code>console_logs</code></strong>: what the Run console printed for the Run button (the last six runs of a project).</li>
  <li><strong><code>tail_file</code></strong>: the end of a log file, whole records only; it returns an offset to read only what was appended next time. A read of a file follows the access rules.</li>
</ul>
<p>Every answer lists what it left out and how to ask for it. Details of the filters are on the Live services page.</p>

<h2>The live run card</h2>
<p>
  A run that used these tools ends with a <strong>Live run</strong> card: what it started and stopped, what is still running, the requests it sent (each opens the timeline on that request), the scenarios and assertions, the files it changed and what the run cost.
</p>

<h2>The build queue</h2>
<p>
  Builds and test runs take a numbered ticket in a queue shared by every conversation and by the Build button, and are served in order, one at a time per project. A waiting call simply returns its result when its turn has come; cancelling the run cancels the wait. Requests for the same module (or the same test selection) queued before a build starts are served by that one build, each with the same report and a note saying which state of the project it is about. The queue is listed in the Live panel.
</p>
<p>
  <strong>Build isolation</strong> (composer settings menu, fixed when the conversation starts): <em>Shared (queue)</em> is the default; <em>Separate target</em> makes a Cargo project compile in a target folder of its own, so the conversation neither waits nor makes others wait, at the cost of disk space and a full first compile. Other toolchains stay in the queue. A conversation in an isolated copy never queues against the project.
</p>

<h2>Who broke the build</h2>
<p>
  When a build fails, the report says which errors are in files this conversation edited, which are in files another conversation edited last, and which other conversations changed anything since the project last compiled. It is a guess from the edit record, because in Rust an error often shows in one file and comes from a change in another. Bennu leaves a short system note for each of those conversations: it reaches Imber with its next tool result, or with your next message if it is idle, lists the errors without repeating itself and is marked as a heuristic. A file nobody edited in this session is reported as yours.
</p>

<h2>Other conversations about a file</h2>
<p>
  Whether sub-agents are on or not, Imber can ask who owns a file (<code>file_owner</code>: the conversation that reserved or last changed it, whether it is running, its current plan step) and leave that conversation a note (<code>request_change</code>). The note is marked as coming from another agent, not from you, is delivered once, and a run may send only a few.
</p>
