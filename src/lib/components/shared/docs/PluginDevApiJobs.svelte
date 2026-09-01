<script lang="ts">
  import { highlight } from '$lib/utils/diff-formatter';
</script>

<h1>Plugin Development — API: Jobs &amp; Integrations</h1>
<p>APIs for running background processes, defining pipelines, executing blocking shell commands, and interacting with the issue tracker.</p>

<h2>arbor.job — background jobs</h2>
<p>Use <code>arbor.job</code> for long-running or async work. The job runs in a separate OS thread; output is streamed line-by-line to the Jobs panel. Use <code>arbor.terminal.exec()</code> only for short blocking commands.</p>
<table class="shortcuts-table">
  <thead><tr><th>Function</th><th>Description</th></tr></thead>
  <tbody>
    <tr><td><code>arbor.job.spawn(config)</code></td><td>Launch a background job. Returns <code>(JobHandle, nil)</code> on success or <code>(nil, err)</code> on a spawn failure (lock / app-handle). The handle is a Promise with extra <code>.id</code> and <code>:cancel()</code> — it resolves with the on-done context on success and rejects with it on failure. Config: <code>name</code>, <code>command</code>, <code>cwd?</code>, <code>env?</code>, <code>category?</code> (groups jobs into collapsible sections in the overlay), <code>hidden?</code> (boolean — when true the job is excluded from the default Jobs panel listing and the status-bar running badge; revealed by the &quot;Show hidden&quot; toggle), <code>on_done_action?</code> (string — sugar), <code>on_done?</code> (function — sugar), <code>target?</code> (string — route the job and its events to a specific window's feedback host, e.g. <code>"merula"</code>; absent → the main window, which also shows every untagged job)</td></tr>
    <tr><td><code>arbor.job.list()</code></td><td>Returns a Lua table of all job records</td></tr>
    <tr><td><code>arbor.job.cancel(job_id)</code></td><td>Kill a running job (SIGTERM / taskkill /T). No-op if the job has already finished.</td></tr>
  </tbody>
</table>
<pre class="language-lua">{@html highlight(`-- Promise-style: chain :ok / :err on the returned handle.
local job, err = arbor.job.spawn({
  name    = "npm build",
  command = "npm run build",
  cwd     = arbor.repo.current(),
})
if err then
  arbor.notify{ message = "Spawn failed: " .. err, level = "error" }
  return
end
arbor.log.info("started job " .. job.id)
job:ok(function(ctx)  arbor.notify{ message = "Build succeeded ✓", level = "success" } end)
   :err(function(ctx) arbor.notify{ message = "Build failed (exit " .. (ctx.exit_code or -1) .. ")", level = "error" } end)

-- on_done / on_done_action stay as zucchero — they fire alongside the promise.
arbor.job.spawn({
  name           = "Cargo build",
  command        = "cargo build --release",
  cwd            = arbor.repo.current(),
  on_done_action = "my_plugin:build_done",
})
arbor.events.on("my_plugin:build_done", function(ctx)
  arbor.log.info("exit_code=" .. ctx.exit_code)
end)

-- Job sequencing via :ok chain.
local function launch_service()
  local svc = arbor.job.spawn({ name = "Server", command = "./server", category = "Services" })
  if svc then svc:ok(function(_) arbor.notify{ title = "Server stopped", message = "", level = "info" } end) end
end

-- Hidden services owned by a domain-specific panel: the job runs but does
-- not appear in the generic Jobs overlay or the status-bar running badge
-- unless the user toggles "Show hidden". Cancellation still works.
arbor.job.spawn({
  name     = "Tomcat catalina",
  command  = "./catalina.sh run",
  cwd      = repo_dir,
  category = "Services",
  hidden   = true,
})

local build = arbor.job.spawn({ name = "Build", command = "make release", category = "Builds" })
if build then
  build:ok(function(_) launch_service() end)
       :err(function(ctx) arbor.notify{ title = "Build failed", message = "exit " .. (ctx.exit_code or -1), level = "error" } end)
end

-- Inside arbor.async.run you can await sequentially.
arbor.async.run(function()
  local b = arbor.job.spawn({ name = "Build", command = "make", category = "Builds" })
  if not b then return end
  local _, berr = arbor.async.await(b)
  if berr then arbor.log.warn("build failed"); return end
  arbor.job.spawn({ name = "Tests", command = "make test" })
end)`, '.lua')}</pre>

<h2>arbor.pipeline — pipelines</h2>
<p>Define and run multi-stage command pipelines. Results appear in the Pipelines panel (Workflow icon in the Activity Bar). No special permissions required.</p>
<table class="shortcuts-table">
  <thead><tr><th>Function</th><th>Description</th></tr></thead>
  <tbody>
    <tr><td><code>arbor.pipeline.define(config)</code></td><td>Register a pipeline. Config: <code>id</code>, <code>name</code>, <code>description?</code>, <code>icon?</code>, <code>stages[]</code> (each with <code>id</code>, <code>name</code>, <code>steps[]</code>)</td></tr>
    <tr><td><code>arbor.pipeline.run&#123; pipeline_id, cwd? &#125;</code></td><td>Start a pipeline run. Returns <code>(run_id, nil)</code> on success, <code>(nil, err)</code> on failure. Optional <code>cwd</code> overrides the default repo-root working directory</td></tr>
    <tr><td><code>arbor.pipeline.cancel(run_id)</code></td><td>Cancel a running pipeline (stops after the current step)</td></tr>
    <tr><td><code>arbor.pipeline.list()</code></td><td>Return all pipeline definitions registered by this plugin</td></tr>
  </tbody>
</table>

<h2>arbor.http — native HTTP client</h2>
<p>Asynchronous HTTP via the bundled <code>reqwest</code> client — no shell-out, no background job, no <code>curl</code> dependency. The callback fires when the response (or an error) arrives.</p>
<table class="shortcuts-table">
  <thead><tr><th>Function</th><th>Description</th></tr></thead>
  <tbody>
    <tr><td><code>arbor.http.get(url, callback)</code></td><td>GET <code>url</code>. <code>callback(response)</code> receives <code>&#123; ok, status, body, error? &#125;</code>.</td></tr>
    <tr><td><code>arbor.http.get(url, opts, callback)</code></td><td>Same with options: <code>&#123; headers = &#123;...&#125;, timeout_ms = 10000 &#125;</code>.</td></tr>
  </tbody>
</table>
<p>
  Requires the <code>network</code> permission. Set it to a list of allowed
  hostnames in <code>plugin.toml</code> — exact match or registrable suffix
  (<code>"maven.org"</code> permits <code>search.maven.org</code> and itself).
  Use <code>["*"]</code> to allow any host (avoid unless strictly necessary).
</p>
<pre class="language-toml">{@html highlight(`# plugin.toml
[permissions]
network = ["search.maven.org", "api.github.com"]`, '.toml')}</pre>
<pre class="language-lua">{@html highlight(`arbor.http.get(
  "https://search.maven.org/solrsearch/select?q=g:%22org.springframework%22&rows=1&wt=json",
  { timeout_ms = 5000 },
  function(r)
    if not r.ok then
      arbor.log.warn("HTTP " .. r.status .. ": " .. (r.error or ""))
      return
    end
    local data = arbor.json.decode(r.body)
    arbor.log.info("Latest: " .. data.response.docs[1].latestVersion)
  end
)

-- With auth header
arbor.http.get(
  "https://api.github.com/repos/foo/bar/issues",
  { headers = { Authorization = "Bearer " .. token, Accept = "application/vnd.github+json" } },
  function(r) ... end
)`, '.lua')}</pre>

<h2>arbor.terminal.exec — blocking shell</h2>
<p>Requires the <code>terminal</code> permission. Always blocks the calling Lua coroutine — use <code>arbor.job.spawn</code> for anything that may take more than a second.</p>
<pre class="language-lua">{@html highlight(`local r, err = arbor.terminal.exec{ command = "git status --short", cwd = arbor.repo.current() }
if err then
  arbor.log.error("exec failed: " .. err)
  return
end
-- r.exit_code : number
-- r.stdout    : string
-- r.stderr    : string`, '.lua')}</pre>

<h2>arbor.issues — issue tracker</h2>
<p>Provides synchronous Lua wrappers around the Linear and Jira APIs. The active provider for each repo is resolved transparently — the same code works for both trackers. Requires <code>issues = "read"</code> or <code>issues = "write"</code> in <code>[permissions]</code>.</p>
<table class="shortcuts-table">
  <thead><tr><th>Function</th><th>Permission</th><th>Description</th></tr></thead>
  <tbody>
    <tr><td><code>arbor.issues.search(filters?)</code></td><td><code>issues = "read"</code></td><td><strong>Linear-only.</strong> Search issues. Returns an array of issue tables. All filter fields are optional. Pass a number or identifier (e.g. <code>"ENG-42"</code>) in <code>query</code> to find by id. There is no <code>identifier</code> filter — use <code>arbor.issues.lookup</code> for exact-id resolution that also routes to Jira when the active repo is bound to it.</td></tr>
    <tr><td><code>arbor.issues.get(id)</code></td><td><code>issues = "read"</code></td><td><strong>Linear-only.</strong> Fetch by Linear UUID (NOT the human identifier). For "ENG-42"-style lookups use <code>arbor.issues.lookup</code>.</td></tr>
    <tr><td><code>arbor.issues.lookup(identifier)</code></td><td><code>issues = "read"</code></td><td>Routes by the active repo's <code>issue_tracker</code> config (<code>linear</code> or <code>jira</code>). Returns the matching issue table, <code>nil</code> on miss / unconfigured tracker, or <code>(nil, err)</code> on auth failure. Linear: candidates are filtered to the exact identifier match; Jira: hands the key straight to <code>GET /issue/&#123;key&#125;</code>. Use this whenever you have a human key like <code>"PROJ-123"</code>.</td></tr>
    <tr><td><code>arbor.issues.transition(id, status_id)</code></td><td><code>issues = "write"</code></td><td>Move an issue to a new workflow state. Returns updated issue.</td></tr>
    <tr><td><code>arbor.issues.comment(issue_id, body)</code></td><td><code>issues = "write"</code></td><td>Add a comment. Returns the new comment table.</td></tr>
    <tr><td><code>arbor.issues.branch_name(issue)</code></td><td>—</td><td>Pure-computation helper: generates a git branch slug from an issue table.</td></tr>
  </tbody>
</table>
<pre class="language-lua">{@html highlight(`local issues = arbor.issues.search({
  query        = "login",      -- title text OR ticket ID ("42", "ENG-42")
  assigneeMe   = true,
  statusIds    = { "10001", "10002" },   -- Jira status IDs or Linear workflow-state UUIDs
  labelIds     = { "bug" },             -- Jira: label name; Linear: label UUID
  issueTypeIds = { "Bug", "Story" },    -- Jira only (ignored on Linear)
  teamId       = "PROJ",               -- Jira: project key; Linear: team UUID
  limit        = 25,
})

for _, issue in ipairs(issues) do
  print(issue.identifier, issue.title, issue.status.name)
end

-- Transition issue (Jira resolves status ID → workflow transition automatically)
arbor.issues.transition(issue.id, status_id)

-- Add a comment
arbor.issues.comment(issue.id, "Deployed to staging ✓")

-- Branch name slug
local branch = arbor.issues.branch_name(issue)
-- Linear: "arb-123-fix-login-bug"
-- Jira:   "proj-456-fix-login-bug"`, '.lua')}</pre>
