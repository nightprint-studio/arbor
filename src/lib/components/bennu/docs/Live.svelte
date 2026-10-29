<script lang="ts">
  /**
   * Live: managed processes, recipes, scenarios, the unified timeline, correlation ids, and Imber's live tools.
   */
  import Callout from '$lib/components/shared/ui/Callout.svelte';
</script>

<span class="eyebrow">Run</span>
<h1>Live</h1>

<p class="doc-lead">
  Live is the tool window for the services a project runs: start a stack in one go, read what each process prints, send requests to it, follow one request through
  every service's log, and replay the same checks as a scenario — by hand, or through Imber, the assistant. <kbd>Ctrl</kbd> + <kbd>Shift</kbd> + <kbd>F11</kbd>, or the rail button
  beside the Profiler.
</p>

<h2>Processes</h2>
<p>
  A <em>managed process</em> is a long-running command (a Spring service, <code>npm run dev</code>, a Cargo binary) started on behalf of the project. It is not a Run
  configuration: it has no console to type into. Its output is kept in a bounded buffer with timestamps (stdout and stderr, colours removed), it can be asked whether it is
  <em>ready</em>, and stopping it ends the <strong>whole process tree</strong> — on Windows through <code>taskkill /T</code>, elsewhere through the process group — not
  only the launcher.
</p>
<dl class="meta-grid">
  <dt>Ready</dt>
  <dd>Any combination of: a port that accepts connections, a log line matching a regular expression, a <code>http://localhost</code> health URL (any 2xx, or an exact status), a delay. All of them must hold, within a timeout. If the process exits first, the wait fails with its last output.</dd>
  <dt>Caps</dt>
  <dd>At most 8 processes running at once, 24 finished ones kept for their logs, 5 000 log lines and 2 MiB per process, and a lifetime of 30 minutes unless a recipe says otherwise. A declared port that is already taken refuses the start before anything runs.</dd>
  <dt>Cleaning up</dt>
  <dd>Closing a project stops what it started; so does closing Bennu. A backend that dies without cleaning up is covered by the same exit watchdog as Run and Test.</dd>
</dl>
<p>In the list, <kbd>↑</kbd> <kbd>↓</kbd> choose a process and <kbd>Delete</kbd> stops it.</p>

<h2>Recipes</h2>
<p>
  A recipe is a named stack in <code>.arbor/bennu/live/recipes.toml</code>. Services start in dependency order, each after the previous one is ready. <strong>Create recipes
  file</strong> in the Recipes tab writes a starter.
</p>
<pre><code>{`[[recipe]]
name = "shop"
description = "Gateway plus the orders service"
stop_order = ["gateway", "orders"]        # optional; default: reverse of the start order

  [[recipe.service]]
  name = "orders"
  command = "mvn -q spring-boot:run"
  cwd = "orders"                          # relative to the project; may not leave it
  env = { SPRING_PROFILES_ACTIVE = "dev" }
  ports = [8081]
  ready = { port = 8081, log = "Started .*Application", timeout_s = 120 }

  [[recipe.service]]
  name = "gateway"
  command = "npm run dev"
  depends_on = ["orders"]
  ready = { http = "http://localhost:3000/health", status = 200 }`}</code></pre>
<p>
  The file is validated as a whole: names, dependency cycles, regular expressions, loopback-only health URLs, ports declared twice, a working directory outside the project. Every
  problem is listed at once. <code>max_minutes</code> overrides the lifetime cap of a service.
</p>
<Callout variant="warning" title="Running a recipe runs its commands">
  The first run of a recipes file shows every command and asks. The approval is bound to the <strong>content</strong> of the file: change a command and it asks again. It is kept
  for the session only — nothing in the repository can approve itself.
</Callout>

<h2>HTTP to localhost</h2>
<p>
  Imber's <code>http_request</code> reaches services on this machine and nothing else: <code>localhost</code>, <code>127.0.0.1</code> and <code>[::1]</code> over plain
  HTTP. Every request carries an <code>X-Correlation-Id</code> header (unless it names one). Secret headers (<code>Authorization</code>, <code>Cookie</code>,
  <code>Set-Cookie</code>, <code>X-Api-Key</code>) are redacted in what is recorded and shown to the model. A <code>POST</code>, <code>PUT</code>, <code>PATCH</code> or
  <code>DELETE</code> to a port none of the started processes declared asks first.
</p>

<h2>The timeline and correlation ids</h2>
<p>
  The Timeline tab merges what every process printed, each request and its answer, and the start / ready / exit of every process into one chronological stream. Filter by text or
  kind, or paste a <strong>correlation id</strong> to see everything that request touched: the request, the response, and every log line of every process that mentions the
  id, with the stack-trace lines that follow. For that to reach the services' logs they must log the header — typically a request filter that puts it in the logging
  context.
</p>

<h2>Scenarios</h2>
<p>
  A scenario is an ordered list of steps with assertions, in <code>.arbor/bennu/live/scenarios/&lt;name&gt;.toml</code>. It runs without Imber: select it in the
  Scenarios tab and press <kbd>Enter</kbd>. Each step reports pass or fail; a failing step shows the failed assertions and the timeline slice while it ran, and its
  request's correlation id opens the timeline on that request. Execution stops at the first failing step unless the step sets <code>continue_on_fail</code>.
</p>
<pre><code>{`name = "order-flow"
description = "Log in, list orders, expect no errors in the log"

[[step]]
type = "start_recipe"
recipe = "shop"

[[step]]
type = "http"
name = "login"                    # saved: later steps can use it
method = "POST"
url = "http://localhost:8080/api/login"
headers = { "Content-Type" = "application/json" }
body = '{"user":"demo"}'
expect_status = 200
max_ms = 800
  [[step.expect_json]]
  path = "$.token"
  exists = true

[[step]]
type = "http"
url = "http://localhost:8080/api/orders"
headers = { Authorization = "Bearer \${login:$.token}" }
expect_status = 200
expect_body = "orders"

[[step]]
type = "assert_log"
pattern = "ERROR"
present = false
timeout_ms = 500

[[step]]
type = "stop_all"`}</code></pre>
<dl class="meta-grid">
  <dt>Step types</dt>
  <dd><code>start_recipe</code> / <code>stop_recipe</code> (<code>recipe</code>), <code>stop_all</code>, <code>http</code>, <code>wait</code> (<code>ms</code>), <code>assert_log</code>, <code>assert_running</code> (<code>process</code>, <code>running</code>).</dd>
  <dt>HTTP checks</dt>
  <dd><code>expect_status</code>, <code>expect_body</code> (a regular expression), <code>max_ms</code>, and any number of <code>[[step.expect_json]]</code> with a <code>path</code> (<code>$.a.b[0]</code>, <code>$['key']</code>, <code>[-1]</code>) and one or more of <code>exists</code>, <code>equals</code>, <code>matches</code>, <code>gt</code>, <code>lt</code>.</dd>
  <dt>Log checks</dt>
  <dd><code>assert_log</code> looks at lines written since the scenario began, or in the last <code>window_ms</code>. With <code>present = true</code> it waits up to <code>timeout_ms</code> for the line; with <code>present = false</code> it waits that long first and then requires its absence.</dd>
  <dt>Saved values</dt>
  <dd>A named <code>http</code> step is saved. Later steps may write <code>{'${name:$.json.path}'}</code>, <code>{'${name:status}'}</code> or <code>{'${name:body}'}</code> in the URL, a header or the body. A reference to something that is not there fails the step instead of sending a blank.</dd>
</dl>
<p>
  The model — a scenario is a suite, a step a test case, an assertion a check with its own pass or fail — is the one JUnit XML and Playwright specs share, so a scenario
  can later be exported to either. The file format does not change for that.
</p>

<h2>Who owns a process</h2>
<p>Every process records who started it: you (from this panel) or a particular Imber conversation, and the list shows the owner. Stopping from this panel always works, whoever started it. An Imber conversation stops only its own processes; a name or port that another owner holds is refused instead of duplicated or killed, a stack another conversation already runs is attached read-only (status, logs, requests), and a recipes file approved in one conversation is not approved in another. Scenarios run one at a time per project, and a scenario's <code>stop_all</code> stops only the processes of whoever runs it.</p>

<h2>The build queue</h2>
<p>
  Builds, test runs and whole-project checks of a project take a numbered ticket and run one at a time, in the order they were asked for, whoever asks — the Build button or any Imber conversation. When anything is queued the panel shows a <strong>Build queue</strong> section: the run in progress and each waiting ticket ("#2 · waiting behind build of …") with the conversation it belongs to. A waiting ticket can be cancelled with its ✕; the run in progress is stopped from the tool that started it. Requests for the same thing queued before a build starts share that one build.
</p>

<h2>Imber's live tools</h2>
<p>
  Settings › Imber › Tools it may use has a <strong>Live services</strong> switch. When it is on the model can <code>run_start</code>, <code>run_status</code>,
  <code>run_logs</code>, <code>run_stop</code>, <code>wait_ready</code>, <code>list_recipes</code>, <code>run_recipe</code>, <code>http_request</code>,
  <code>trace_request</code>, <code>timeline</code>, <code>list_scenarios</code>, <code>run_scenario</code>, <code>save_scenario</code> and the log readers described below. Reading and inspecting never asks.
  Starting a process asks every time; running a recipe or a scenario asks once per content of its file; stopping what the run started does not ask. The processes are the same
  ones the Live panel lists.
</p>
<p>
  A run that used these tools ends with a <strong>Live run</strong> card: what it started and stopped, what is still running, the requests it sent (each opens the timeline on
  that request), the scenarios and assertions, the files it changed, and what the run cost.
</p>

<h2>Reading logs without reading them</h2>
<p>
  A log is read as <strong>records</strong>: a line together with its stack trace, the <code>Caused by</code> chain, a Rust backtrace or a multi-line statement. The levels, timestamps and exceptions are read by the same interpreter that colours the Run console, from logback / log4j / Spring lines, Rust <code>env_logger</code> and <code>tracing</code>, logfmt and JSON logs. The same readers work on a managed process, on the output of the Run console and on a log file.
</p>
<ul>
  <li><strong><code>run_logs</code></strong> — a process's records, newest 80 by default. <code>summary</code> gives the counts per level, the first distinct errors (with first time and count) and the last error; <code>level</code> (at least that severity), <code>pattern</code> (a regex over the whole record), <code>last</code> (<code>30s</code>, <code>5m</code>), <code>after</code> / <code>before</code> (ISO-8601 or epoch milliseconds; no offset means UTC), <code>context</code> (records around each match, merged, gaps shown as <code>--</code>) and <code>frames</code> (stack lines per record; the last <code>Caused by</code> is always kept) narrow it. Identical consecutive records are folded (<code>x12</code>). <code>next_since</code> returns only what is new.</li>
  <li><strong><code>wait_log</code></strong> — waits for a new line matching a regex (default 30 s, at most 300) and returns it with three records of context on each side; it returns early, with the last output, if the process exits.</li>
  <li><strong><code>console_logs</code></strong> — what the <em>Run console</em> printed for the Run button: lists the recent runs (<code>r3</code>, configuration, start, status) and reads one with the same filters. The backend keeps the last six runs of a project, up to 5,000 lines or 2 MB each, and forgets them when the project closes.</li>
  <li><strong><code>tail_file</code></strong> — the end of a log file (512 KB by default, at most 4 MB), whole records only. It returns <code>next_offset</code>; passing it back as <code>since_offset</code> gives only what was appended, and a file that shrank (rotated or truncated) is read again from its end, and says so. The encoding is detected. It is a read of that file under the access rules.</li>
  <li><strong><code>log_facets</code></strong> — a compact digest instead of lines, for a process, <code>console</code> or <code>file:&lt;path&gt;</code>. <em>Exceptions</em> are grouped by class, root cause and the first frame inside the project (shown as <code>file:line</code>); <em>queries</em> (Hibernate, p6spy, <code>JdbcTemplate</code>, MyBatis, sqlx) have their literals replaced and come slowest first when the log carries times; <em>requests</em> (access logs, tower / axum traces) are grouped by endpoint with their statuses, failing endpoints first; <em>warnings</em> are grouped by message template. Each group has a count, first and last time and a sample record id. <code>facet</code> shows one facet in depth, <code>limit</code> raises the groups listed, and <code>where</code> filters its occurrences (<code>status &gt;= 500</code>, <code>duration &gt; 200ms and sql contains orders</code>, <code>class contains Order</code>).</li>
  <li><strong><code>log_record</code></strong> — one record in full, by the id a digest printed, optionally with the records around it.</li>
</ul>
<p>
  Every answer lists what it left out (older records, hidden stack lines, records without a level or a timestamp, lines the process's buffer dropped) and how to ask for it.
</p>

<h2>Command palette</h2>
<p>
  <em>Toggle Live</em>, <em>Live: recipes</em>, <em>Live: scenarios</em>, <em>Live: timeline</em> and <em>Live: stop all processes</em>.
</p>
