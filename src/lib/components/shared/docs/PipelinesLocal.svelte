<script lang="ts">
  import { highlight } from '$lib/utils/diff-formatter';
</script>

<h1>Pipelines — Plugin Pipelines</h1>
<p>Arbor's pipeline system lets plugins define and run multi-stage CI/CD-style workflows directly inside the app. Each pipeline is a sequence of <strong>stages</strong>, each containing one or more <strong>steps</strong> (shell commands). Progress is shown in a live node graph.</p>

<h2>Opening the Pipelines panel</h2>
<p>Click the <strong>Workflow</strong> icon in the Activity Bar (bottom group). Toggle the panel to show/hide it as a resizable bottom section. The two sub-views (<strong>Local Pipelines</strong> — plugin-defined — and <strong>CI / CD</strong> — GitHub Actions / GitLab CI) are inline tabs in the panel header next to the title.</p>

<h2>Panel layout</h2>
<p>The Local Pipelines tab is a two-column IntelliJ-style Run window:</p>
<ul>
  <li>
    <strong>Left toolbar</strong> (36 px column) — global pipeline-level actions:
    a primary <strong>Run</strong> button that re-launches the most recently
    launched pipeline (sticky), then icon-only <strong>Stop all running</strong>,
    <strong>Resume last failed</strong>, and <strong>Clear history</strong>
    (terminal runs only). To launch a different pipeline, right-click one
    of its run cards in the list — the context menu has a Run entry that
    fires the same routed launch flow. Plugins can contribute additional
    toolbar buttons via the <code>arbor:pipelines:toolbar</code> contribution
    point.
  </li>
  <li>
    <strong>Right column</strong> — a filter row with a multi-select dropdown
    (<em>All pipelines</em> by default) and a live run-count summary, then the
    scrollable run list below it. Each card shows status pill, duration, the
    pipeline-definition badge (with an <strong>orphan</strong> tag when the
    def is no longer registered), step count, and a timestamp.
  </li>
</ul>

<h2>Running a pipeline</h2>
<p>Click the <strong>Run</strong> icon in the left toolbar to replay the most recently launched pipeline. To run a different one, right-click any of its existing run cards in the list and pick <strong>Run “…”</strong> from the context menu — the menu's other entries (Open detail, Cancel, Resume, Discard) mirror the row's hover buttons. The orchestrator spawns a background thread that executes each step sequentially. The node graph updates in real time with status colours:</p>
<p>
  <strong>Self-contained replay vs. plugin-routed launch.</strong> A
  pipeline def with non-empty <code>stages</code> is treated as
  self-contained: every step has its command / op / cwd already resolved
  (variable substitution baked in by whatever flow produced it — combo
  button, sequence runner, …), so Play replays it directly via
  <code>arbor.pipeline.run</code> without involving the owning plugin.
  This means a def compiled in a previous tab keeps replaying correctly
  from the panel even after the user switches repos.
</p>
<p>
  A def with empty <code>stages</code> is a <em>stub</em> the plugin
  registered upfront so the panel has something to show on first open.
  Stubs cannot be replayed verbatim — Play asks the owning plugin to
  materialise stages via the <code>on_pipeline_run_request</code> hook
  (typically by compiling a profile or resolving a build configuration)
  and the plugin then calls <code>arbor.pipeline.run</code> itself. If a
  plugin registers stubs but doesn't implement the hook, Play surfaces a
  clear error pointing the user to the plugin's own launch UI.
</p>
<table class="shortcuts-table">
  <thead><tr><th>Colour</th><th>Meaning</th></tr></thead>
  <tbody>
    <tr><td>Green</td><td>Success — step / stage / run completed with exit code 0</td></tr>
    <tr><td>Red</td><td>Failed — non-zero exit code (pipeline stops unless <code>allow_failure = true</code>)</td></tr>
    <tr><td>Blue (accent)</td><td>Running — currently executing</td></tr>
    <tr><td>Grey</td><td>Pending / Cancelled</td></tr>
  </tbody>
</table>

<h2>Viewing step output</h2>
<p>Click any step node in the graph to expand an output pane at the bottom of the detail area. It shows captured <code>stdout</code> and <code>stderr</code> (stderr lines are highlighted in red). Up to 1 000 lines are retained per step.</p>

<h2>Cancellation, resume and discard</h2>
<p>The cancel/resume/discard affordances live in two places: per-card icon buttons on the right of each run row (cancel for running, resume for failed, trash for terminal), and the bulk equivalents in the left toolbar (<strong>Stop all running</strong>, <strong>Resume last failed</strong>, <strong>Clear history</strong>). All of them are also reachable from Lua via <code>arbor.pipeline.cancel(run_id)</code>, <code>arbor.pipeline.resume(run_id)</code> and <code>arbor.pipeline.discard(run_id)</code>. Cancellation stops the pipeline after the <em>current step</em> finishes — it does not kill a running process mid-execution.</p>
<p>
  When a step fails (non-zero exit code, <code>allow_failure=false</code>), the
  run enters status <code>failed</code> but remains <strong>resumable</strong>:
  its output, log buffer and a <code>resume_cursor</code> pointing at the exact
  failing steps are persisted to disk. Call
  <code>arbor.pipeline.resume(run_id)</code> (or use the Resume button in the
  UI) to restart the run from that cursor — already-successful steps are
  skipped, only the failed ones are re-executed. A resume requires the
  pipeline's lock to be free.
</p>
<p>
  Use <code>arbor.pipeline.discard(run_id)</code> to drop a terminal run
  permanently (removes the persisted JSON file). Discard refuses to act on a
  <code>running</code> run — cancel it first.
</p>

<h2>Concurrency &amp; locking</h2>
<p>
  Every pipeline has a <code>lock_key</code> (default
  <code>"&lt;plugin&gt;:&lt;id&gt;"</code>). Only one run per lock key may be in
  <code>running</code> state at a time — a second attempt fails immediately with
  a descriptive log entry. <strong>Terminal runs (failed / cancelled / success)
  do NOT hold the lock</strong>: they remain resumable but a new run of the same
  pipeline can start freely. When another run is active, a resume of an older
  failed run is rejected until the active one finishes.
</p>
<p>
  You can check lock state with
  <code>local owner = arbor.pipeline.is_locked(lock_key)</code> which returns
  the <code>run_id</code> currently holding the lock, or <code>nil</code> when
  free. Override the default key by passing <code>lock_key = "..."</code> to
  <code>arbor.pipeline.define</code> — useful when different pipelines compete
  for the same external resource (e.g. a deploy target).
</p>

<h2>Parallel steps inside a stage</h2>
<p>
  Stages are always executed <strong>sequentially</strong> (top-to-bottom), but
  inside a stage steps can run in parallel. Set <code>mode = "parallel"</code>
  on the stage and optionally cap concurrency with
  <code>max_parallel = N</code>. All steps of a parallel stage are awaited
  before the next stage starts; an early failure doesn't cancel its siblings
  (GitLab-CI semantics). Resume re-runs only the failing step(s) of a parallel
  stage, leaving the successful ones alone.
</p>

<h2>Logging &amp; log level</h2>
<p>
  The orchestrator auto-logs pipeline / stage / step lifecycle events. Each
  run has its own capped log buffer (5 000 entries) plus a live stream via the
  <code>arbor://pipeline-log</code> event. Events are filtered by the run's
  configured <code>log_level</code> (default <code>info</code>) — set
  <code>log_level = "debug"</code> on <code>arbor.pipeline.define</code> to
  also capture the per-line step output and resolved parameters. Available
  levels: <code>debug</code>, <code>info</code>, <code>warn</code>,
  <code>error</code>.
</p>

<h2>Defining pipelines from a plugin</h2>
<p>Two equivalent shapes — pick whichever reads better for your case:</p>
<ul>
  <li><code>arbor.pipeline.define(table)</code> — declarative table config (good when you build the pipeline programmatically from data).</li>
  <li><code>arbor.pipeline("id"):...:commit()</code> — chainable builder (good for static, hand-written pipelines). Compiles down to the same table on <code>:commit()</code>.</li>
</ul>

<h3>Builder DSL</h3>
<pre class="language-lua"><code>{@html highlight(`arbor.pipeline("build")
  :name("Build & Test")
  :description("Compile, lint and run unit tests")
  :icon("Hammer")
  :lock("my-plugin:build")
  :log_level("info")
  :stage("Prepare")
    :shell("npm install")
  :stage("Verify"):mode("parallel"):max_parallel(2)
    :shell({ id = "lint", name = "Lint", command = "npm run lint", allow_failure = true })
    :shell({ id = "test", name = "Unit tests", command = "npm test" })
  :commit()`, '.lua')}</code></pre>
<p>
  Builder methods: <code>:name</code> · <code>:description</code> ·
  <code>:icon</code> · <code>:lock</code> (alias <code>:lock_key</code>) ·
  <code>:log_level</code> · <code>:stage(name|cfg)</code> · <code>:mode</code> ·
  <code>:max_parallel</code> · <code>:run(op, params)</code> ·
  <code>:shell(cmd|cfg)</code> · <code>:step(cfg)</code> · <code>:commit()</code>.
  Steps go to the most recently opened stage; <code>:run</code> takes
  <code>(op_name, params)</code> or a single <code>&#123;op, params, plugin?, id?, name?, allow_failure?&#125;</code>
  table; <code>:shell</code> takes a string or a <code>&#123;command, cwd?, ...&#125;</code> table.
  Step ids default to <code>s1</code>, <code>s2</code>, ... when omitted.
</p>

<h3>Table config</h3>
<p>Equivalent to the builder above; call from your plugin's <code>on_plugin_load</code> handler (or at module level):</p>
<pre class="language-lua"><code>{@html highlight(`arbor.pipeline.define({
  id          = "build",
  name        = "Build & Test",
  description = "Compile, lint and run unit tests",
  icon        = "🔨",
  log_level   = "info",              -- debug | info | warn | error
  lock_key    = "my-plugin:build",   -- optional; default "<plugin>:<id>"
  stages = {
    {
      id   = "prepare",
      name = "Prepare",
      -- mode defaults to "sequential"
      steps = {
        {
          id      = "install",
          name    = "Install dependencies",
          command = "npm install",
        },
      },
    },
    {
      id           = "verify",
      name         = "Verify",
      mode         = "parallel",   -- lint + test run concurrently
      max_parallel = 2,            -- optional cap (omit = unlimited)
      steps = {
        {
          id             = "lint",
          name           = "Lint",
          command        = "npm run lint",
          allow_failure  = true,
        },
        {
          id      = "test",
          name    = "Unit tests",
          command = "npm test",
        },
      },
    },
  },
})`, '.lua')}</code></pre>

<h2>Running a pipeline from Lua</h2>
<pre class="language-lua"><code>{@html highlight(`-- Start a run; returns (run_id, nil) on success or (nil, err) on failure.
local run_id, err = arbor.pipeline.run{ pipeline_id = "build" }

-- Override the working directory for all steps:
local run_id, err = arbor.pipeline.run{ pipeline_id = "build", cwd = "/path/to/project" }

-- Cancel a running pipeline (stops after the current step):
arbor.pipeline.cancel(run_id)

-- Resume a failed run from the steps that halted it.
-- Returns (false, err) when the lock is held by another run.
local ok, err = arbor.pipeline.resume(run_id)

-- Drop a terminal run (removes the persisted JSON file).
local ok, err = arbor.pipeline.discard(run_id)

-- Check who holds the concurrency lock (nil when free).
local owner = arbor.pipeline.is_locked("my-plugin:build")

-- List definitions registered by this plugin:
local defs = arbor.pipeline.list()

-- Look one up by id (scoped to this plugin); returns nil when missing.
-- Useful in re-define paths to inherit the existing display name set by
-- a previous stub registration.
local def = arbor.pipeline.get("build")
if def then arbor.log.info("currently named: " .. def.name) end`, '.lua')}</code></pre>

<h2>Toolbar contribution (<code>arbor:pipelines:toolbar</code>)</h2>
<p>
  Plugins can add extra icon-only buttons to the panel's left toolbar.
  Contribute to <code>arbor:pipelines:toolbar</code> with a payload describing
  a single button; the host renders one button per active contribution and
  fires the action when the user clicks it. Use it for plugin-specific
  ops like "Re-run failed steps", "Open Source Export profile", "View dashboard".
</p>
<pre class="language-lua"><code>{@html highlight(`-- main.lua
arbor.contribute("arbor:pipelines:toolbar", {
  payload = {
    icon            = "Zap",          -- lucide icon name
    tooltip         = "Re-run failed steps from the current filter",
    accent          = false,           -- optional: use accent color
    success         = false,           -- optional: green tint
    danger          = false,           -- optional: red tint on hover
    divider_before  = false,           -- optional: 1px separator above the button
    disabled        = false,           -- optional
  },
  action = function(ctx)
    -- Your plugin's logic here. The toolbar is non-modal, so prefer
    -- a notify+job pattern (toasting "Started…") over a blocking call.
  end,
})`, '.lua')}</code></pre>
<p>
  Buttons appear after the built-in Run / Stop / Resume / Clear cluster, in
  registration order. The host swallows errors thrown by the action so a
  buggy plugin can't break the toolbar.
</p>

<h2>Pipeline hooks</h2>
<p>Declare hooks in <code>[hooks]</code> in your <code>plugin.toml</code> and register handlers with <code>arbor.events.on()</code>:</p>
<table class="shortcuts-table">
  <thead><tr><th>Constant</th><th>TOML key</th><th>Context fields</th></tr></thead>
  <tbody>
    <tr><td><code>"on_pipeline_run_request"</code></td><td><code>on_pipeline_run_request</code></td><td><code>pipeline_id, tab_id?</code> — fired on the def's owning plugin when the user presses Play on a <em>stub</em> def (empty <code>stages</code>). Defs with non-empty stages are replayed directly without invoking this hook. The handler must compile stages and call <code>arbor.pipeline.run</code> itself.</td></tr>
    <tr><td><code>"on_pipeline_started"</code></td><td><code>on_pipeline_started</code></td><td><code>run_id, pipeline_id, plugin</code></td></tr>
    <tr><td><code>"on_pipeline_step_done"</code></td><td><code>on_pipeline_step_done</code></td><td><code>run_id, pipeline_id, plugin, stage_id, step_id, step_name, status, exit_code</code></td></tr>
    <tr><td><code>"on_pipeline_done"</code></td><td><code>on_pipeline_done</code></td><td><code>run_id, pipeline_id, plugin, status</code></td></tr>
  </tbody>
</table>
<pre class="language-lua"><code>{@html highlight(`-- Map the panel's Play click back into the plugin's own launch flow.
-- The id we registered (e.g. "profile:abc") encodes whatever lookup key
-- the plugin needs.
arbor.events.on("on_pipeline_run_request", function(ctx)
  local def_id = ctx.pipeline_id or ""
  if def_id:sub(1, 8) ~= "profile:" then return end
  local profile = pcfg.find(def_id:sub(9))
  if not profile then return end
  compile.run(profile)        -- materialises stages then arbor.pipeline.run
end)`, '.lua')}</code></pre>
<pre class="language-toml"><code>{@html highlight(`-- plugin.toml
[hooks]
on_pipeline_started   = true
on_pipeline_step_done = true
on_pipeline_done      = true`, 'toml')}</code></pre>
<pre class="language-lua"><code>{@html highlight(`arbor.events.on("on_pipeline_done", function(ctx)
  if ctx.status == "success" then
    arbor.notify{ title = "Pipeline done", message = ctx.pipeline_id .. " succeeded", level = "success" }
  else
    arbor.notify{ title = "Pipeline failed", message = ctx.pipeline_id .. " — status: " .. ctx.status, level = "error" }
  end
end)`, '.lua')}</code></pre>

<h2>Pipeline options</h2>
<table class="shortcuts-table">
  <thead><tr><th>Field</th><th>Type</th><th>Description</th></tr></thead>
  <tbody>
    <tr><td><code>id</code></td><td>string</td><td>Unique pipeline identifier within the plugin</td></tr>
    <tr><td><code>name</code></td><td>string</td><td>Human-readable label</td></tr>
    <tr><td><code>description</code></td><td>string?</td><td>Tooltip on the Run dropdown entry and the per-card definition badge</td></tr>
    <tr><td><code>icon</code></td><td>string?</td><td>Emoji or icon identifier</td></tr>
    <tr><td><code>lock_key</code></td><td>string?</td><td>Concurrency key. Default <code>"&lt;plugin&gt;:&lt;id&gt;"</code></td></tr>
    <tr><td><code>log_level</code></td><td>string?</td><td><code>debug</code> | <code>info</code> (default) | <code>warn</code> | <code>error</code></td></tr>
    <tr><td><code>stages</code></td><td>array</td><td>Array of <code>StageDef</code></td></tr>
  </tbody>
</table>

<h2>Stage options</h2>
<table class="shortcuts-table">
  <thead><tr><th>Field</th><th>Type</th><th>Description</th></tr></thead>
  <tbody>
    <tr><td><code>id</code></td><td>string</td><td>Unique stage identifier within the pipeline</td></tr>
    <tr><td><code>name</code></td><td>string</td><td>Label</td></tr>
    <tr><td><code>mode</code></td><td>string?</td><td><code>sequential</code> (default) | <code>parallel</code></td></tr>
    <tr><td><code>max_parallel</code></td><td>integer?</td><td>Cap concurrency when <code>mode=parallel</code>. Omit = unlimited</td></tr>
    <tr><td><code>steps</code></td><td>array</td><td>Array of <code>StepDef</code></td></tr>
  </tbody>
</table>

<h2>Step options</h2>
<p>
  A step is one of four <strong>kinds</strong>, picked by which field is set
  (precedence top-to-bottom): <code>if_block</code> →
  <code>builtin</code> → <code>lua_op</code> → <code>command</code>. The
  remaining fields (cwd / env / allow_failure / capture) apply across kinds.
</p>
<table class="shortcuts-table">
  <thead><tr><th>Field</th><th>Type</th><th>Description</th></tr></thead>
  <tbody>
    <tr><td><code>id</code></td><td>string</td><td>Unique step identifier within the stage</td></tr>
    <tr><td><code>name</code></td><td>string</td><td>Human-readable label shown in the graph node</td></tr>
    <tr><td><code>command</code></td><td>string?</td><td>Shell command (run via <code>sh -c</code> / <code>cmd /C</code>). <code>$&#123;var&#125;</code> references are resolved before the process spawns.</td></tr>
    <tr><td><code>lua_op</code></td><td>table?</td><td>Invoke a plugin-registered Lua handler instead of spawning a shell. Shape: <code>&#123; op = "name", params = &#123;...&#125;, plugin? = "..." &#125;</code>. <code>$&#123;var&#125;</code> in <code>params</code> string fields is resolved before dispatch.</td></tr>
    <tr><td><code>builtin</code></td><td>table?</td><td>Run a built-in op (file_exists / file_read / env / json_get / path_join / set_var / echo / match). See the dedicated section below. Resolved by the runtime — no shell, no Lua VM.</td></tr>
    <tr><td><code>if_block</code></td><td>table?</td><td>Conditional control step. Evaluates each branch's condition in order and runs the chosen branch's nested steps. See <em>If / elif / else blocks</em>.</td></tr>
    <tr><td><code>cwd</code></td><td>string?</td><td>Working directory. <code>nil</code> = active repo root. <code>$&#123;var&#125;</code> resolved.</td></tr>
    <tr><td><code>env</code></td><td>table?</td><td>Extra env vars for shell steps. <code>$&#123;var&#125;</code> resolved per value.</td></tr>
    <tr><td><code>allow_failure</code></td><td>bool</td><td>If <code>true</code>, the stage continues even if this step fails. Default: <code>false</code></td></tr>
    <tr><td><code>capture</code></td><td>table?</td><td>After the step finishes, extract a value from its outcome and store it in the run's variable bag. See <em>Variables &amp; capture</em>.</td></tr>
  </tbody>
</table>

<h2>Variables &amp; capture</h2>
<p>
  Every pipeline run owns a typed <strong>variable bag</strong> (empty at
  start). Steps populate it via <code>capture</code>; later steps
  reference its values via <code>$&#123;var&#125;</code> syntax in any string field —
  <code>command</code>, <code>cwd</code>, <code>env</code> values,
  <code>lua_op.params</code>, <code>builtin</code> params, and
  <code>if_block</code> conditions all run through the same resolver
  before they execute. <code>$$</code> escapes a literal <code>$</code>;
  <code>$&#123;name:-fallback&#125;</code> supplies a default for missing names.
</p>
<p>
  A <code>capture</code> spec has three pieces:
</p>
<ul>
  <li><code>var</code> — name to store under (no <code>$</code> prefix).</li>
  <li><code>source</code> — what part of the step's outcome to capture:
    <code>"stdout"</code> (default), <code>"stderr"</code>,
    <code>"exit_code"</code>, <code>"success"</code> (boolean: exit_code == 0),
    or <code>"return_value"</code> (Lua/builtin's typed return — falls back
    to stdout for shell steps).</li>
  <li><code>transforms</code> — optional ordered list of <em>declarative
    transforms</em> applied left-to-right to massage the captured value
    before storing it.</li>
</ul>
<table class="shortcuts-table">
  <thead><tr><th>Transform</th><th>Effect</th></tr></thead>
  <tbody>
    <tr><td><code>&#123; kind="trim" &#125;</code></td><td>Strip leading/trailing whitespace</td></tr>
    <tr><td><code>&#123; kind="lower" &#125;</code> · <code>&#123; kind="upper" &#125;</code></td><td>ASCII case folding</td></tr>
    <tr><td><code>&#123; kind="lines" &#125;</code></td><td>Split a string on <code>\n</code> → list (drops trailing empty lines)</td></tr>
    <tr><td><code>&#123; kind="split", sep="," &#125;</code></td><td>Split on a literal separator → list</td></tr>
    <tr><td><code>&#123; kind="join", sep=", " &#125;</code></td><td>Join a list with <code>sep</code> → string</td></tr>
    <tr><td><code>&#123; kind="first" &#125;</code> · <code>&#123; kind="last" &#125;</code> · <code>&#123; kind="nth", n=2 &#125;</code></td><td>Index a list (negative <code>n</code> counts from end)</td></tr>
    <tr><td><code>&#123; kind="regex", pattern="v(\\d+)", group=1 &#125;</code></td><td>Match a regex; with <code>group</code> returns that captured group</td></tr>
    <tr><td><code>&#123; kind="matches_bool", pattern="^OK" &#125;</code></td><td>Same as <code>regex</code> but returns a boolean</td></tr>
    <tr><td><code>&#123; kind="json_parse" &#125;</code> · <code>&#123; kind="json_get", path="a.b.0" &#125;</code></td><td>Parse a JSON string; walk a dotted path</td></tr>
    <tr><td><code>&#123; kind="to_bool" &#125;</code> · <code>&#123; kind="to_number" &#125;</code></td><td>Coerce to boolean / number (<code>null</code> on failure)</td></tr>
    <tr><td><code>&#123; kind="default", value="N/A" &#125;</code></td><td>Replace empty / null with a fallback string</td></tr>
  </tbody>
</table>
<p>
  Failures inside a transform chain don't fail the step — the variable
  becomes <code>null</code> and the trace is logged. Use the run log
  panel (debug level) to see each transform's input/output preview.
</p>
<pre class="language-lua"><code>{@html highlight(`-- Capture the first version line emitted by 'mvn -v' and store
-- it as \${maven_version} for later steps.
{
  id      = "detect-mvn",
  name    = "Detect Maven",
  command = "mvn -v",
  capture = {
    var    = "maven_version",
    source = "stdout",
    transforms = {
      { kind = "lines" },
      { kind = "first" },
      { kind = "regex", pattern = "Apache Maven (\\\\d+\\\\.\\\\d+\\\\.\\\\d+)", group = 1 },
    },
  },
}

-- Use it in a downstream shell step:
{ id = "log", name = "Log version", command = "echo 'Building with mvn \${maven_version}'" }`, '.lua')}</code></pre>

<h2>Built-in ops</h2>
<p>
  Built-in ops are tiny side-effect-free helpers the runtime executes
  directly — no shell, no Lua VM. Use them mostly to seed the variable
  bag (with <code>capture</code>) so <code>if_block</code> conditions
  and later steps can branch on file presence, environment vars,
  parsed JSON fields, and so on.
</p>
<table class="shortcuts-table">
  <thead><tr><th>Kind</th><th>Fields</th><th>Returns</th></tr></thead>
  <tbody>
    <tr><td><code>file_exists</code></td><td><code>path</code></td><td><code>bool</code></td></tr>
    <tr><td><code>file_read</code></td><td><code>path</code>, <code>max_bytes?</code></td><td><code>string</code> (file contents)</td></tr>
    <tr><td><code>env</code></td><td><code>name</code>, <code>default?</code></td><td><code>string</code> (env var or default)</td></tr>
    <tr><td><code>json_get</code></td><td><code>source</code> (JSON string), <code>path</code></td><td>typed value at the dotted path</td></tr>
    <tr><td><code>path_join</code></td><td><code>parts</code> (array of strings)</td><td><code>string</code></td></tr>
    <tr><td><code>set_var</code></td><td><code>value</code> (any JSON)</td><td>the value verbatim — pair with <code>capture.var</code></td></tr>
    <tr><td><code>echo</code></td><td><code>message</code></td><td><code>string</code> (also written to the run log)</td></tr>
    <tr><td><code>match</code></td><td><code>target</code>, <code>pattern?</code> (substring) or <code>regex?</code></td><td><code>bool</code></td></tr>
  </tbody>
</table>
<pre class="language-lua"><code>{@html highlight(`-- Capture whether 'docker-compose.yml' exists into a flag.
{
  id      = "check-compose",
  name    = "Detect compose file",
  builtin = { kind = "file_exists", path = "docker-compose.yml" },
  capture = { var = "has_compose", source = "return_value" },
}`, '.lua')}</code></pre>

<h2>If / elif / else blocks</h2>
<p>
  An <code>if_block</code> step is a <em>control step</em>: instead of
  running a command, the orchestrator evaluates each branch's condition
  in order and runs the chosen branch's nested <code>steps</code>. The
  child outcomes appear under the parent step in the run viewer
  (<code>StepRun.children</code>) and the picked branch label
  (<code>"if"</code>, <code>"elif #1"</code>, <code>"else"</code>) lands in
  <code>StepRun.branch</code>. The step's overall status is
  <code>success</code> when every chosen child succeeded (honoring
  <code>allow_failure</code>) and <code>failed</code> otherwise.
</p>
<p>
  Conditions are <strong>structured values</strong> — there's no DSL or
  parser. Each leaf is a small object with a <code>kind</code> tag and
  the operands it needs. Operands are <code>$&#123;var&#125;</code>-resolved before
  comparison.
</p>
<table class="shortcuts-table">
  <thead><tr><th>Kind</th><th>Fields</th><th>Notes</th></tr></thead>
  <tbody>
    <tr><td><code>compare</code></td><td><code>left</code>, <code>op</code>, <code>right</code></td><td><code>op</code> ∈ <code>eq</code>, <code>ne</code>, <code>i_eq</code>, <code>contains</code>, <code>starts_with</code>, <code>ends_with</code>, <code>matches</code> (right = regex), <code>gt</code>/<code>lt</code>/<code>gte</code>/<code>lte</code> (numeric)</td></tr>
    <tr><td><code>truthy</code></td><td><code>value</code></td><td>True for non-empty / non-zero / non-"false". A bare <code>"$&#123;var&#125;"</code> reference uses the variable's typed truthiness.</td></tr>
    <tr><td><code>defined</code></td><td><code>var</code></td><td>True when the variable is present and not <code>null</code>.</td></tr>
    <tr><td><code>empty</code></td><td><code>value</code></td><td>True when the resolved value is the empty string.</td></tr>
    <tr><td><code>all_of</code> · <code>any_of</code> · <code>not</code></td><td><code>conditions</code> / <code>condition</code></td><td>Logical combinators.</td></tr>
    <tr><td><code>always</code> · <code>never</code></td><td>—</td><td>Constants. <code>always</code> is the natural condition for the catch-all <code>else</code> branch (or just leave <code>else_steps</code> set).</td></tr>
  </tbody>
</table>
<pre class="language-lua"><code>{@html highlight(`-- Build differently depending on whether a 'pom.xml' is present.
{
  id   = "smart-build",
  name = "Smart build",
  if_block = {
    branches = {
      {
        condition = { kind = "compare",
                      left = "\${has_pom}", op = "eq", right = "true" },
        steps = {
          { id = "mvn", name = "mvn package", command = "mvn -B package" },
        },
      },
      {
        condition = { kind = "compare",
                      left = "\${has_gradle}", op = "eq", right = "true" },
        steps = {
          { id = "gw", name = "gradlew build", command = "./gradlew build" },
        },
      },
    },
    else_steps = {
      { id = "fail", name = "No build tool", command = "exit 1",
        allow_failure = false },
    },
  },
}`, '.lua')}</code></pre>
<p>
  Nested <code>if_block</code> steps inside a branch's <code>steps</code>
  work — drilling deep is supported, the run viewer shows the
  parent/child tree, and resume from a failure re-runs the entire
  parent <code>if_block</code> (re-evaluating the condition on the
  fresh variable bag).
</p>

<p>
  <strong>Pipeline editor.</strong> The generic
  <code>PluginPipelineEditor</code> component supports drilling into an
  if-block step via the small "open" arrow on its row — the breadcrumb
  above the sequence column tracks the path and lets the user pop back
  with one click. Plugins drive this by implementing the
  <code>enter_step</code> action (push current location onto a stack,
  re-emit a filtered <code>stages</code> list and a <code>breadcrumb</code>)
  and <code>navigate_to</code> (pop back to a given level).
</p>

<h2>LuaOp steps</h2>
<p>
  A <strong>LuaOp</strong> step calls a Lua function registered by a plugin
  instead of spawning a process. This is the right choice when you need
  structured file edits (JSON / YAML / TOML / XML), want access to the
  <code>arbor.*</code> API from within a step, or simply want to skip the
  shell round-trip for performance / portability.
</p>
<p>Register a handler, then reference it from a step:</p>
<pre class="language-lua"><code>{@html highlight(`-- Register once (typical: in on_plugin_load)
arbor.pipeline.register_op("bump-config", function(params, ctx)
  -- params is the table from the step; ctx.cwd is the step's working dir.
  arbor.fs.json_set{ path = params.path, jpath = "$.version", value = params.version }
  return { exit_code = 0, stdout = "bumped " .. params.path }
end)

-- Use it in a pipeline def:
arbor.pipeline.define({
  id = "deploy", name = "Deploy", stages = {
    { id = "s1", name = "Bump",
      steps = {
        {
          id   = "b1", name = "Bump config.json",
          lua_op = { op = "bump-config",
                     params = { path = "config.json", version = "2.0.0" } },
        },
      } },
  },
})`, '.lua')}</code></pre>
<p>Handler return shapes (all accepted):</p>
<ul>
  <li><code>nil</code> / <code>true</code> → exit_code = 0 (success)</li>
  <li><code>false</code> → exit_code = 1</li>
  <li><code>&lt;number&gt;</code> → that exit code</li>
  <li><code>&lt;string&gt;</code> → stdout, exit_code = 0</li>
  <li><code>&#123; exit_code?, stdout?, stderr? &#125;</code> → structured</li>
</ul>
<p>Raising an error fails the step with the message captured in stdout/stderr.</p>

<h2>Built-in op catalog (<code>arbor.core.*</code>)</h2>
<p>
  Two ready-made op modules ship inside every plugin sandbox: structured
  edits and assertions. They cover the bulk of pipeline plumbing — opt in
  per module; each one a plugin doesn't <code>require</code> stays unloaded.
  File / text ops aren't shipped here: they're trivial wrappers over
  <code>arbor.fs</code> / <code>arbor.text</code>, so plugins keep a local
  copy when they need them (see <code>plugins/source-export/pipeline_ops/</code>
  for the canonical reference).
</p>
<table class="shortcuts-table">
  <thead><tr><th>Module</th><th>Ops</th></tr></thead>
  <tbody>
    <tr>
      <td><code>arbor.core.edit</code></td>
      <td><code>json_edit</code>, <code>yaml_edit</code>, <code>toml_edit</code>, <code>xml_edit</code></td>
    </tr>
    <tr>
      <td><code>arbor.core.assert</code></td>
      <td><code>assert_file_exists</code>, <code>assert_file_not_contains</code>, <code>assert_glob_matches</code>, <code>assert_version_bump</code></td>
    </tr>
  </tbody>
</table>
<p>
  Every op has the signature <code>function(params, ctx) -&gt; &#123; exit_code, stdout, stderr? &#125;</code>
  and logs structured trace lines on stdout (<code>[op_name] key = value</code>)
  that the pipeline panel renders verbatim.
</p>
<p>Two usage patterns — pick whichever fits:</p>
<pre class="language-lua"><code>{@html highlight(`-- Pattern 1: register every op in the module so pipeline
-- StepDefs can refer to them by bare name.
arbor.events.on("on_plugin_load", function()
  require("arbor.core.assert").register()
end)

arbor.pipeline.define({
  id = "deploy", name = "Deploy", stages = {
    { id = "verify", name = "Verify", steps = {
      { id = "war-exists", name = "WAR present",
        lua_op = { op = "assert_file_exists",
                   params = { path = "target/app.war" } } },
    } },
  },
})

-- Pattern 2: cherry-pick a single op without registering the whole module.
local assert_glob_matches = require("arbor.core.assert").assert_glob_matches
arbor.pipeline.register_op("assert_glob_matches", assert_glob_matches)

-- Plugin-local op for everything else — wrap arbor.fs / arbor.text directly:
arbor.pipeline.register_op("delete_war", function(params, ctx)
  local p = arbor.fs.join(ctx.cwd, params.path)
  if arbor.fs.exists(p) then arbor.fs.delete(p) end
  return { exit_code = 0, stdout = "removed " .. p }
end)`, '.lua')}</code></pre>
<p>
  Permissions: every op routes filesystem access through <code>arbor.fs.*</code>,
  so the calling plugin's own <code>fs</code> level (<code>"none"</code> /
  <code>"read"</code> / <code>"write"</code>) and <code>fs_scope</code>
  apply. Requiring <code>arbor.core.assert</code> in a sandboxed plugin does NOT
  grant extra access.
</p>

<h2>Structured file edits (arbor.fs.*_set)</h2>
<p>
  Rust-backed helpers available from inside a LuaOp handler (or anywhere else
  with <code>fs_write</code> permission):
</p>
<table class="shortcuts-table">
  <thead><tr><th>Function</th><th>Backend</th><th>Path syntax</th></tr></thead>
  <tbody>
    <tr><td><code>arbor.fs.json_set&#123; path, jpath, value, pretty? &#125;</code></td><td><code>serde_json</code></td><td><code>$.foo.bar</code>, <code>foo.bar</code>, <code>items.0.name</code>, <code>servers[1].host</code></td></tr>
    <tr><td><code>arbor.fs.yaml_set&#123; path, ypath, value &#125;</code></td><td><code>serde_yaml</code> → json walker</td><td>dotted path, same as JSON</td></tr>
    <tr><td><code>arbor.fs.toml_set&#123; path, tpath, value &#125;</code></td><td><code>toml</code> crate</td><td>dotted path; comments are NOT preserved on rewrite</td></tr>
    <tr><td><code>arbor.fs.xml_set&#123; path, xpath, value &#125;</code></td><td><code>quick-xml</code></td><td>minimal XPath: <code>/a/b/c</code>, <code>//c</code>, <code>/a/@attr</code>, <code>/a/b[@k='v']/c</code></td></tr>
  </tbody>
</table>
<p>
  Intermediate nodes are auto-created for missing keys. <code>value</code> can
  be any serialisable Lua value (string / number / boolean / table) for JSON /
  YAML / TOML; XML takes a string (attribute value or element text).
</p>

<h2>Live log stream</h2>
<p>
  Subscribe to <code>arbor://pipeline-log</code> from the frontend (or via
  <code>arbor.events.on</code> in Lua) to receive log events as they happen.
  Payload shape: <code>&#123; run_id, ts, level, scope, message &#125;</code>
  where <code>scope</code> is <code>"pipeline"</code>,
  <code>"stage:&lt;stage_id&gt;"</code> or
  <code>"step:&lt;stage_id&gt;.&lt;step_id&gt;"</code>. Only events at or above
  the run's <code>log_level</code> are emitted.
</p>

<h2>Permissions</h2>
<p>No special permissions are required to define or trigger pipelines — any plugin can call <code>arbor.pipeline.define()</code> and <code>arbor.pipeline.run()</code>. The commands run under the same OS user as Arbor itself. Plugins do <em>not</em> need the <code>terminal</code> permission for pipeline steps (that applies only to <code>arbor.terminal.exec</code>).</p>
