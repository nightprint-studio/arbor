<script lang="ts">
  import { highlight } from '$lib/utils/diff-formatter';
  import { RefreshCw, RotateCcw, ExternalLink } from 'lucide-svelte';
</script>

<h1>Pipelines</h1>
<p>Arbor's pipeline system lets plugins define and run multi-stage CI/CD-style workflows directly inside the app. Each pipeline is a sequence of <strong>stages</strong>, each containing one or more <strong>steps</strong> (shell commands). Progress is shown in a live node graph.</p>

<h2>Opening the Pipelines panel</h2>
<p>Click the <strong>Workflow</strong> icon in the Activity Bar (bottom group). It is always visible. Toggle the panel to show/hide it as a resizable bottom section. Inside, two tabs are available: <strong>Local Pipelines</strong> (plugin-defined) and <strong>CI / CD</strong> (GitHub Actions / GitLab CI).</p>

<h2>Running a pipeline</h2>
<p>Select a pipeline in the left sidebar, then click <strong>Run</strong>. The orchestrator spawns a background thread that executes each step sequentially. The node graph updates in real time with status colours:</p>
<table class="shortcuts-table">
  <thead><tr><th>Colour</th><th>Meaning</th></tr></thead>
  <tbody>
    <tr><td>Green</td><td>Success â€” step / stage / run completed with exit code 0</td></tr>
    <tr><td>Red</td><td>Failed â€” non-zero exit code (pipeline stops unless <code>allow_failure = true</code>)</td></tr>
    <tr><td>Blue (accent)</td><td>Running â€” currently executing</td></tr>
    <tr><td>Grey</td><td>Pending / Cancelled</td></tr>
  </tbody>
</table>

<h2>Viewing step output</h2>
<p>Click any step node in the graph to expand an output pane at the bottom of the detail area. It shows captured <code>stdout</code> and <code>stderr</code> (stderr lines are highlighted in red). Up to 1 000 lines are retained per step.</p>

<h2>Cancellation</h2>
<p>Click the <strong>Ã—</strong> button next to a running run in the run list, or call <code>arbor.pipeline.cancel(run_id)</code> from Lua. Cancellation stops the pipeline after the <em>current step</em> finishes â€” it does not kill a running process mid-execution. A run that's still <em>queued</em> behind the global concurrency cap can also be cancelled the same way: it ends in <code>cancelled</code> without ever transitioning through <code>running</code>.</p>

<h2>Global concurrency cap</h2>
<p>Arbor caps the number of pipeline runs that may be <code>running</code> simultaneously across <em>all</em> plugins. Additional runs queue up in <code>pending</code> state with a <strong>Queued</strong> badge in the Pipelines panel and start as soon as a slot frees up.</p>
<p>Edit the cap from <strong>Settings â†’ Tools â†’ Pipelines</strong> â€” changes apply within ~250 ms (no restart). The value is also persisted in <code>~/.config/arbor/config.toml</code>:</p>
<pre class="language-toml"><code>{@html highlight(`[pipelines]
# Max concurrent pipeline runs. 0 = unlimited.
max_concurrent_runs = 4`, 'toml')}</code></pre>
<p>Notes:</p>
<ul>
  <li>The cap applies only to <em>local</em> pipelines (this orchestrator). CI/CD runs on GitHub Actions / GitLab CI are scheduled by the provider and ignore this setting.</li>
  <li>The per-stage <code>max_parallel</code> still applies <em>inside</em> a single run (parallel steps within one stage); the global cap controls how many runs may overlap.</li>
  <li>The <code>lock_key</code> collision rule is unchanged: trying to start a second run for a busy lock fails immediately, regardless of how much queue room there is.</li>
  <li><code>0</code> means unlimited â€” the orchestrator never queues. Useful for benchmarks but a burst of pipelines (sequence runs, group dashboards, large fan-outs) can saturate disk I/O and the libgit2 packfile readers; the Settings page surfaces a warning when this mode is selected.</li>
</ul>

<h2>Defining pipelines from a plugin</h2>
<p>Call <code>arbor.pipeline.define(config)</code> in your plugin's <code>on_plugin_load</code> handler (or at module level):</p>
<pre class="language-lua"><code>{@html highlight(`arbor.pipeline.define({
  id          = "build",
  name        = "Build & Test",
  description = "Compile, lint and run unit tests",
  icon        = "ðŸ”¨",   -- optional emoji shown in the sidebar
  silent      = false,  -- optional, default false. When true, the host
                        -- skips the automatic "started" toast and the
                        -- "succeeded/failed/cancelled" bell notification
                        -- for runs of this pipeline. Use it when your
                        -- plugin already surfaces equivalent messages.
  stages = {
    {
      id   = "prepare",
      name = "Prepare",
      steps = {
        {
          id      = "install",
          name    = "Install dependencies",
          command = "npm install",
          -- cwd defaults to the active repo root
        },
      },
    },
    {
      id   = "verify",
      name = "Verify",
      steps = {
        {
          id             = "lint",
          name           = "Lint",
          command        = "npm run lint",
          allow_failure  = true,   -- pipeline continues even if this fails
        },
        {
          id      = "test",
          name    = "Unit tests",
          command = "npm test",
          cwd     = nil,           -- nil = active repo root
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

-- Per-run silence override (default inherited from the def).
-- Pass true to mute the host's start-toast / done-notification for this
-- specific run, or false to force them when the def is normally silent.
local run_id, err = arbor.pipeline.run{ pipeline_id = "build", silent = true }

-- Cancel a running pipeline:
arbor.pipeline.cancel(run_id)

-- List definitions registered by this plugin:
local defs = arbor.pipeline.list()
for _, d in ipairs(defs) do
  arbor.log.info(d.id .. ": " .. d.name)
end`, '.lua')}</code></pre>

<h2>Automatic start / done notifications</h2>
<p>The host watches every pipeline run and fires two built-in notifications so
the user doesn't have to keep the Pipelines panel open:</p>
<ul>
  <li>A <strong>transient toast</strong> when the run transitions to <code>running</code> ("Pipeline X started"), with an inline <em>Open</em> button that deep-links to the run's detail modal.</li>
  <li>A <strong>persistent bell notification</strong> when the run reaches a terminal state â€” <code>success</code>, <code>failed</code> or <code>cancelled</code>. Click <em>Open</em> to jump straight to the graph + step output.</li>
</ul>
<p>Silence both surfaces for a given pipeline by passing <code>silent = true</code> to
<code>arbor.pipeline.define</code>, or override per run with
<code>arbor.pipeline.run&#123; silent = true &#125;</code>. Plugins that already raise
their own lifecycle messages (build started/finished, deploy succeeded, â€¦)
should mark their pipelines silent to avoid duplicated cards.</p>

<h2>Pipeline hooks</h2>
<p>Declare hooks in <code>[hooks]</code> in your <code>plugin.toml</code> and register handlers with <code>arbor.events.on()</code>:</p>
<table class="shortcuts-table">
  <thead><tr><th>Constant</th><th>TOML key</th><th>Context fields</th></tr></thead>
  <tbody>
    <tr><td><code>"on_pipeline_started"</code></td><td><code>on_pipeline_started</code></td><td><code>run_id, pipeline_id, plugin</code></td></tr>
    <tr><td><code>"on_pipeline_step_done"</code></td><td><code>on_pipeline_step_done</code></td><td><code>run_id, pipeline_id, plugin, stage_id, step_id, step_name, status, exit_code</code></td></tr>
    <tr><td><code>"on_pipeline_done"</code></td><td><code>on_pipeline_done</code></td><td><code>run_id, pipeline_id, plugin, status</code></td></tr>
  </tbody>
</table>

<pre class="language-toml"><code>{@html highlight(`-- plugin.toml
[hooks]
on_pipeline_started   = true
on_pipeline_step_done = true
on_pipeline_done      = true`, 'toml')}</code></pre>

<pre class="language-lua"><code>{@html highlight(`-- main.lua
arbor.events.on("on_pipeline_started", function(ctx)
  arbor.log.info("Pipeline started: " .. ctx.pipeline_id)
end)

arbor.events.on("on_pipeline_step_done", function(ctx)
  if ctx.status == "failed" then
    arbor.notify{ title = "Step failed", message = ctx.step_name .. " exited " .. tostring(ctx.exit_code), level = "error" }
  end
end)

arbor.events.on("on_pipeline_done", function(ctx)
  if ctx.status == "success" then
    arbor.notify{ title = "Pipeline done", message = ctx.pipeline_id .. " succeeded", level = "success" }
  else
    arbor.notify{ title = "Pipeline failed", message = ctx.pipeline_id .. " â€” status: " .. ctx.status, level = "error" }
  end
end)`, '.lua')}</code></pre>

<h2>Step options</h2>
<table class="shortcuts-table">
  <thead><tr><th>Field</th><th>Type</th><th>Description</th></tr></thead>
  <tbody>
    <tr><td><code>id</code></td><td>string</td><td>Unique step identifier within the stage</td></tr>
    <tr><td><code>name</code></td><td>string</td><td>Human-readable label shown in the graph node</td></tr>
    <tr><td><code>command</code></td><td>string</td><td>Shell command (run via <code>sh -c</code> / <code>cmd /C</code>)</td></tr>
    <tr><td><code>cwd</code></td><td>string?</td><td>Working directory. <code>nil</code> = active repo root</td></tr>
    <tr><td><code>allow_failure</code></td><td>bool</td><td>If <code>true</code>, the stage continues even if this step fails. Default: <code>false</code></td></tr>
  </tbody>
</table>

<h2>Permissions</h2>
<p>No special permissions are required to define or trigger pipelines â€” any plugin can call <code>arbor.pipeline.define()</code> and <code>arbor.pipeline.run()</code>. The commands run under the same OS user as Arbor itself. To execute shell commands within a step, the plugin does <em>not</em> need <code>terminal</code> permissions (those apply only to <code>arbor.terminal.exec</code>).</p>

<h2>GitHub Actions &amp; GitLab CI</h2>
<p>The <strong>CI / CD</strong> tab in the Pipelines panel connects to GitHub Actions or GitLab CI and shows real pipeline runs fetched directly from the API. This works for any repository whose remote URL points to <code>github.com</code> or a GitLab instance.</p>

<h3>Authentication</h3>
<p>An OAuth token is required. Connect your account in <strong>Settings â†’ Authentication</strong> before using the CI tab:</p>
<ul>
  <li><strong>GitHub Actions</strong> â€” connect your GitHub account via Device Flow. Arbor requests the <code>repo</code> + <code>read:user</code> scopes.</li>
  <li><strong>GitLab CI</strong> â€” connect via GitLab Device Flow. Arbor requests the <code>api</code> + <code>read_user</code> scopes. Self-hosted instances use a host-based credential stored with <strong>Settings â†’ Credentials</strong>.</li>
</ul>
<p>If no token is found, the CI tab shows a banner directing you to Settings rather than an error.</p>

<h3>CI run list</h3>
<p>Each row in the CI run list shows:</p>
<ul>
  <li>A <strong>status pill</strong> (Passed / Failed / Running / Cancelled / Pending) with colour coding.</li>
  <li>The <strong>wall-clock duration</strong> (computed from API timestamps).</li>
  <li>The <strong>workflow / pipeline name</strong> and its provider ID.</li>
  <li>The <strong>branch chip</strong> (accent colour) and short <strong>commit SHA</strong>.</li>
  <li>A human-readable <strong>time-ago</strong> label.</li>
</ul>
<p>Click anywhere on a run card to open the <strong>Pipeline Detail</strong> modal.</p>

<h3>Pipeline detail modal</h3>
<p>Clicking a run opens a full-screen modal showing:</p>
<ul>
  <li>Header: provider icon, run name, branch/commit/duration chips, status badge.</li>
  <li>A <strong>stage/job graph</strong> â€” horizontal columns, one per stage (GitLab) or "Jobs" (GitHub). Each column lists job cards with their status icon, name, and duration. Clicking a job card opens its log page in the browser.</li>
  <li>Jobs with <code>allow_failure: true</code> are shown slightly dimmed with an <strong>!</strong> badge when they fail.</li>
  <li><strong>Re-run</strong> and <strong>Open in browser</strong> buttons in the modal header.</li>
</ul>
<p>For GitLab, jobs are grouped by their native <code>stage</code> name. For GitHub, all jobs appear in a single "Jobs" column since GitHub Actions does not expose a first-class stage concept in the jobs API.</p>

<h3>Creating a new pipeline run</h3>
<p>Click the <strong>Run</strong> button in the CI / CD header (only visible when a token is configured) to open the <em>New Pipeline Run</em> modal:</p>
<ul>
  <li><strong>Branch</strong> â€” dropdown pre-filled with the current HEAD branch. All local branches are listed.</li>
  <li><strong>Workflow</strong> (GitHub only) â€” dropdown listing active workflows that have <code>on: workflow_dispatch</code> configured. If no dispatch-enabled workflows are found, a hint is shown.</li>
  <li><strong>Variables</strong> â€” dynamic key/value table. Add as many variables as needed; blank-key rows are ignored on submit. For GitLab these become <code>env_var</code> variables; for GitHub they become <code>workflow_dispatch</code> inputs.</li>
</ul>
<p>After clicking <strong>Run Pipeline</strong>:</p>
<ul>
  <li><strong>GitLab</strong> â€” the new pipeline is created synchronously and the run list refreshes immediately.</li>
  <li><strong>GitHub</strong> â€” a <code>workflow_dispatch</code> event is fired (HTTP 204). GitHub queues the run asynchronously, so the list refreshes automatically after a 3-second delay.</li>
</ul>

<h3>What you can do</h3>
<table class="shortcuts-table">
  <thead><tr><th>Action</th><th>How</th></tr></thead>
  <tbody>
    <tr><td>View recent runs</td><td>Switch to the <strong>CI / CD</strong> tab â€” the last 30 runs are fetched automatically</td></tr>
    <tr><td>Create a new run</td><td>Click the <strong>Run</strong> button in the CI header â€” opens branch/variable picker</td></tr>
    <tr><td>Refresh the list</td><td>Click the <RefreshCw size={11} /> button in the panel header</td></tr>
    <tr><td>View stage/job graph</td><td>Click any run card to open the detail modal</td></tr>
    <tr><td>Re-trigger a run</td><td>Click <RotateCcw size={11} /> in the run card or inside the detail modal</td></tr>
    <tr><td>Open run in browser</td><td>Click <ExternalLink size={11} /> in the run card or modal header</td></tr>
    <tr><td>Open a specific job's logs</td><td>Click a job card inside the detail modal</td></tr>
  </tbody>
</table>

<h3>Run status mapping</h3>
<table class="shortcuts-table">
  <thead><tr><th>Arbor status</th><th>GitHub</th><th>GitLab</th></tr></thead>
  <tbody>
    <tr><td>âœ… Passed</td><td><code>completed / success</code></td><td><code>success</code>, <code>passed</code></td></tr>
    <tr><td>âŒ Failed</td><td><code>completed / failure</code>, <code>timed_out</code></td><td><code>failed</code></td></tr>
    <tr><td>â³ Running</td><td><code>in_progress</code>, <code>queued</code></td><td><code>running</code></td></tr>
    <tr><td>â­• Cancelled</td><td><code>completed / cancelled</code>, <code>skipped</code></td><td><code>canceled</code>, <code>skipped</code></td></tr>
    <tr><td>ðŸ”µ Pending</td><td><code>waiting</code>, <code>requested</code></td><td><code>pending</code>, <code>created</code>, <code>scheduled</code></td></tr>
  </tbody>
</table>

<h3>Self-hosted GitLab</h3>
<p>Self-hosted GitLab instances are auto-detected from the remote URL (any host containing <code>gitlab.</code>). Store a personal access token via <strong>Settings â†’ Credentials</strong> using the instance hostname as the key. Arbor will use that token for all API calls to that host.</p>
