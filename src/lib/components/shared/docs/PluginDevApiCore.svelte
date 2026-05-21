<script lang="ts">
  import { highlight } from '$lib/utils/diff-formatter';
</script>

<h1>Plugin Development â€” API: Core</h1>
<p>Core Lua APIs available to all plugins. No special permissions required unless noted.</p>

<h2>Calling convention</h2>
<p>The <code>arbor.*</code> API uses two consistent conventions throughout:</p>
<ul>
  <li><strong>Errors as tuples</strong>. Any function that can fail at runtime (I/O, parse, network, git, registry) returns <code>(value, nil)</code> on success and <code>(nil, err_string)</code> on failure. Callers that don't care about the error simply read the first return value; callers that do care can check the second. Programming errors (permission denied, missing required argument, wrong Lua type) still raise â€” those are bugs to fix in the plugin, not recoverable failure modes.</li>
  <li><strong>Table arguments for &gt; 2 args or any optional arg</strong>. Functions like <code>arbor.fs.move&#123; src, dest, overwrite? &#125;</code>, <code>arbor.terminal.exec&#123; command, cwd? &#125;</code>, <code>arbor.text.replace&#123; content, pattern, replacement, plain? &#125;</code> take a single config table. This keeps call sites readable when fields are added later. Single-mandatory-arg functions (<code>arbor.fs.read(path)</code>, <code>arbor.repo.remote(name)</code>) stay positional; <code>arbor.events.emit(name, payload)</code> is also positional as a hot-path exception.</li>
</ul>
<pre class="language-lua">{@html highlight(`-- Tuple return: ignore the error or branch on it
local content    = arbor.fs.read(path)                  -- nil on failure, fine if you don't care
local content, err = arbor.fs.read(path)                 -- handle the failure explicitly
if not content then arbor.log.warn("read: " .. err); return end

-- Table-config: missing required field RAISES (programming error)
local ok, err = arbor.fs.move{ src = a, dest = b, overwrite = true }
if not ok then arbor.log.warn("move failed: " .. err) end`, '.lua')}</pre>


<h2>arbor.log â€” logging</h2>
<pre class="language-lua">{@html highlight(`arbor.log.debug("detailed trace")
arbor.log.info("something happened")
arbor.log.warn("unexpected state: " .. tostring(val))
arbor.log.error("fatal: " .. err)
-- All messages are prefixed [plugin-name] in the Arbor log`, '.lua')}</pre>

<p>
  Every call is also pushed to an in-memory ring buffer (last 5 000 entries)
  and surfaced in the <strong>Plugin Logs</strong> bottom panel â€”
  <em>Tools â†’ Plugin Logs</em> in the main menu, or <kbd>Alt+Shift+L</kbd>.
  Disabled plugins do not log: their entries are dropped at the API boundary,
  and plugins disabled at startup never get a Lua VM in the first place.
</p>

<h3>Plugin Logs panel</h3>
<p>
  The panel streams new lines in real time and is the canonical place to
  triage plugin behaviour without leaving Arbor.
</p>
<ul>
  <li>
    <strong>Multi-select plugin filter</strong> â€” a Filter dropdown with one
    checkbox per plugin that has logged anything this session. Includes
    <em>All plugins</em> / <em>None</em> shortcuts and a header summary
    (<em>"compile-action +2"</em>) when more than one is active.
  </li>
  <li>
    <strong>Per-level toggles</strong> â€” independent buttons for
    <code>debug</code> / <code>info</code> / <code>warn</code> /
    <code>error</code>. Off levels are excluded from the visible list and
    the line counter.
  </li>
  <li>
    <strong>Free-text search</strong> â€” case-insensitive substring match
    across the whole formatted line (timestamp, level, plugin, message).
    The search field highlights matches inline.
  </li>
  <li>
    <strong>Pipeline tagging</strong> â€” log lines mirrored from a pipeline
    step's captured stdout/stderr carry the pipeline name and run id. A
    dedicated <em>Pipeline</em> selector in the filter dropdown lets you
    isolate one run; <em>Clear pipeline logs</em> wipes only those entries
    and leaves your direct <code>arbor.log.*</code> output intact.
  </li>
  <li>
    <strong>Structured highlighting</strong> â€” recognised tokens
    (timestamps, run ids, exit codes, paths) get their own colour so a
    scrolling stream stays scannable. Severity tints follow the global
    palette (info / warn / error).
  </li>
  <li>
    <strong>Auto-scroll &amp; jump-to-latest</strong> â€” the panel pins the
    view to the newest line; scrolling up pauses auto-follow and reveals a
    pill that snaps back to the bottom on click.
  </li>
  <li>
    <strong>Copy &amp; Clear</strong> â€” Copy serialises the currently
    visible (i.e. filtered) lines to the clipboard as plain text. Clear
    drops every entry from the buffer.
  </li>
</ul>
<p>
  The 5 000-entry cap evicts oldest-first. If you need durable
  per-plugin retention, write to your own log file via
  <code>arbor.fs.append</code>.
</p>

<h2>arbor.settings â€” persistence</h2>
<p>Settings are split into two scopes:</p>
<ul>
  <li><strong>global</strong> â€” stored in <code>~/.config/arbor/plugin_data/&lt;name&gt;/global.json</code> â€” independent of the active repo</li>
  <li><strong>project</strong> â€” stored in <code>&lt;repo&gt;/.arbor/plugins/&lt;name&gt;/project.json</code> â€” per-repository; raises a Lua error if no repo is open</li>
</ul>
<pre class="language-lua">{@html highlight(`-- Global settings
arbor.settings.global.set("api_key", "secret")
local key  = arbor.settings.global.get("api_key")     -- nil if absent
local all  = arbor.settings.global.get_all()            -- table of all keys
arbor.settings.global.clear("api_key")                 -- delete a single key (set to nil)

-- Project settings (requires an active repo)
arbor.settings.project.set("profile", "prod")
local p = arbor.settings.project.get("profile")
local all_proj = arbor.settings.project.get_all()`, '.lua')}</pre>

<h2>arbor.json â€” encode / decode</h2>
<pre class="language-lua">{@html highlight(`local s, err = arbor.json.encode({ key = "val", n = 42 })
-- s = '{"key":"val","n":42}'   err = nil on success

local t, err = arbor.json.decode('{"a":1}')
-- t.a == 1   err = nil on success`, '.lua')}</pre>

<h2>arbor.json_studio â€” open the JSON inspector</h2>
<p>One-call API that opens a host-rendered modal: lazy virtualised tree, JSONPath query, syntax-highlighted text view. Pass <code>text</code> or <code>path</code>. Backed by simd-json on the host so multi-megabyte payloads stay responsive. Earmarked to migrate to a self-contained WASM plugin once that runtime lands â€” the API will not change.</p>
<pre class="language-lua">{@html highlight(`-- Open from disk (host reads the file)
arbor.json_studio.open({ path = "/abs/data.json" })

-- Open inline text
arbor.json_studio.open({
  text  = response_body,
  title = "API response",   -- optional; defaults to filename or "JSON Studio"
})

-- The query bar in the modal accepts full RFC 9535 JSONPath:
--   $.foo.bar                       -- property chain
--   $.arr[0]   $.arr[1:5]            -- index / slice
--   $..key                           -- recursive descent
--   $.users[?@.age > 30]             -- filter
--   $.books[?@.price < 10 && @.in_stock]
--   $..*[?match(@.email, ".*@.*")]   -- regex (RFC function)
--   $[?length(@.tags) > 2]           -- length() / count()
-- Plus shorthands: bare "foo" â†’ $..foo, ".foo" â†’ $.foo, etc.`, '.lua')}</pre>

<h2>arbor.fs â€” filesystem</h2>
<p>Requires the <code>fs</code> permission: <code>"read"</code> for read-only ops, <code>"write"</code> for read+write. The <code>fs_scope</code> field controls path bounds â€” empty (default) sandboxes to the active repo; <code>["*"]</code> grants unrestricted access; any other list extends the active-repo sandbox with those absolute paths. All read/write functions return <code>result, nil</code> on success or <code>nil, err</code> on failure.</p>
<pre class="language-lua">{@html highlight(`local content, err = arbor.fs.read("/path/to/file.txt")
local ok,      err = arbor.fs.write("/path/to/out.txt", content)
local entries      = arbor.fs.list("/path/to/dir")  -- array of {name, is_file, is_dir}
local joined       = arbor.fs.join("/base", "sub", "file.txt")
local exists       = arbor.fs.exists("/path")
local is_file      = arbor.fs.is_file("/path")
local is_dir       = arbor.fs.is_dir("/path")
-- copy(src, dst): if dst is an existing dir, file is placed inside it
arbor.fs.copy("/path/to/app.war", "/opt/tomcat/webapps/")
-- delete(path): removes a file or a directory tree
arbor.fs.delete("/path/to/old.war")`, '.lua')}</pre>

<h2>arbor.repo â€” repository info</h2>
<p>Read functions require <code>git = "read"</code> (or higher). <code>fetch_active_tab</code> and <code>clone</code> require <code>git = "write"</code> (or higher).</p>
<pre class="language-lua">{@html highlight(`local path     = arbor.repo.current()           -- active repo path, or nil
local branch   = arbor.repo.branch()            -- current branch name
local dirty    = arbor.repo.is_dirty()          -- bool: uncommitted changes?
local remote   = arbor.repo.remote("origin")    -- URL of the named remote, or nil

-- Fetch origin for the currently active tab (the tab the user is looking at).
-- Returns true on success, false when silently skipped (no active tab, no
-- origin remote, or network failure â€” no error is raised either way).
-- After a successful fetch, emits "arbor://graph-refresh" so the frontend
-- reloads the commit graph and remote branch list automatically.
-- Ideal for use inside a focus-gated scheduler (only_when_focused = true).
local ok = arbor.repo.fetch_active_tab()   -- requires git = "write" (or higher)

-- List branches and tags of the active repo (sorted, with is_head flag).
local branches = arbor.repo.branches()         -- [{name, is_remote, is_head}]
local tags     = arbor.repo.tags()             -- [{name, target}]

-- List commits in a range, newest-first by author time. Returns (commits, err).
-- Defaults: from=nil (walk to root), to="HEAD", limit=1000, include_merges=true.
local commits, err = arbor.repo.commits({
  from           = "v1.0.0",   -- exclusive lower bound (commit/tag/branch)
  to             = "HEAD",     -- inclusive upper bound
  limit          = 500,
  include_merges = false,
})
-- Each commit: { oid, short_oid, summary, message, author_name,
--                author_email, author_time, parents }

-- List untracked-and-not-ignored paths in the working tree.
-- Useful for housekeeping plugins (e.g. proposing .gitignore entries).
local paths = arbor.repo.untracked()           -- ["target/foo.bin", ".env", ...]`, '.lua')}</pre>

<h3>arbor.repo.clone â€” background clone</h3>
<p>
  Clone a remote repository into a local directory. The clone runs in a
  background <strong>Job</strong> â€” progress streams into the Jobs overlay and
  Job Output panel exactly like <code>arbor.job.spawn</code> results, with
  live cancel support. Uses the system <code>git</code> binary so SSH keys and
  credential helpers (including the Arbor keyring) work transparently.
</p>
<p>Returns the <code>job_id</code> string, so you can pair it with <code>arbor.job.list()</code> / <code>arbor.job.cancel(id)</code>.</p>
<pre class="language-lua">{@html highlight(`local job_id = arbor.repo.clone({
  url                = "https://github.com/user/repo.git",  -- required
  dest               = "/abs/path/to/target",               -- required, parent dir must exist
  branch             = "main",                              -- optional (--branch)
  shallow            = false,                               -- optional (--depth 1)
  recurse_submodules = false,                               -- optional (--recurse-submodules)
  name               = "Clone myrepo",                      -- optional display name in Jobs overlay
  category           = "Clone",                             -- optional grouping label
  on_done            = function(ctx)
    -- ctx = { job_id, success, exit_code, cancelled, dest, url }
    if ctx.success then
      arbor.notify{ title = "Clone done", message = ctx.dest, level = "success" }
    else
      arbor.notify{ title = "Clone failed", message = "exit " .. tostring(ctx.exit_code), level = "error" }
    end
  end,
})   -- requires git = "write" (or higher)`, '.lua')}</pre>

<h2>arbor.workspace â€” workspace and repo-registry queries</h2>
<p>
  Read-only APIs for inspecting the user's workspaces and the central repo
  registry. No special permissions required. The mutating <code>switch()</code>
  call emits <code>arbor://workspace-switched</code> and fires the
  <code>on_workspace_switched</code> hook so other plugins can react.
</p>
<pre class="language-lua">{@html highlight(`local list   = arbor.workspace.list()          -- [{id, name, color_idx, group_id, repo_ids, repo_count}]
local active = arbor.workspace.active()         -- active workspace or nil
local ws     = arbor.workspace.get(ws_id)       -- single workspace or nil

-- Every repo Arbor has ever registered (not just the active workspace's members).
local all_repos = arbor.workspace.list_repos()   -- [{id, path, display_name, remote_url}]
-- Just the repos in a specific workspace:
local ws_repos  = arbor.workspace.list_repos(ws_id)

local repo = arbor.workspace.repo(repo_id)       -- {id, path, display_name, remote_url} or nil

-- Activate a different workspace (swaps the tab set on the UI side).
local ok = arbor.workspace.switch(ws_id)         -- returns bool`, '.lua')}</pre>

<h2>arbor.tabs â€” programmatic tab control</h2>
<p>
  Open a registered repository as an Arbor tab. The repo must already be
  in the registry (added via the workspace UI or auto-registered via
  <code>arbor.workspace.list_repos</code>). If a tab for that repo is
  already open, it is brought to the front instead of duplicated.
</p>
<pre class="language-lua">{@html highlight(`local ok, err = arbor.tabs.open_repo(repo_id)   -- (true, nil) | (false, err)`, '.lua')}</pre>

<h2>arbor.mr / arbor.ci â€” git provider MRs &amp; CI (credential-blind)</h2>
<p>
  Read-only access to merge requests and CI runs hosted on the git
  provider behind a registered repository. Permission gate:
  <code>provider = "read"</code>. The OAuth token never leaves the OS
  keyring; the host resolves it internally and hands the plugin only
  the decoded payloads. Pass <code>repo_id</code> from
  <code>arbor.workspace.list_repos()</code> to scope the call to a
  specific registered repo, or omit it to use the active tab.
</p>
<pre class="language-lua">{@html highlight(`-- Who am I on this provider?
local me = arbor.mr.current_user({ repo_id = entry.id })   -- { id, login, name, ... }

-- List my open MRs across one repo. Use the literal "current_user"
-- sentinel to mean "the authenticated user on THIS provider" â€” the host
-- resolves it for you, the plugin never has to know the actual login.
local mrs, err = arbor.mr.list({
  repo_id = entry.id,        -- workspace registry id; default: active repo
  state   = "open",          -- "open" | "closed" | "merged" | "all"
  author  = "current_user",  -- or any explicit login string
})
-- Each MR (camelCase): { number, title, state, isDraft, author, sourceBranch,
--                        targetBranch, webUrl, checksStatus, ... }

-- Most recent CI run on a branch
local runs, err = arbor.ci.runs({
  repo_id  = entry.id,
  branch   = mr.sourceBranch,
  per_page = 1,
})
-- Each run: { id, name, status, branch, commit_sha, web_url, created_at,
--             provider, duration_secs }`, '.lua')}</pre>

<h2>arbor.security â€” vulnerability dashboard (credential-blind)</h2>
<p>
  Read-only access to GitLab Vulnerability Reports and GitHub
  GHAS / Dependabot / Secret-Scanning posture data. Same permission
  gate (<code>provider = "read"</code>) and same <code>repo_id</code>
  resolution as <code>arbor.mr</code> / <code>arbor.ci</code>. Default
  state filter is active-only (Detected + Confirmed) â€” pass
  <code>states</code> explicitly for closed findings or both.
</p>
<pre class="language-lua">{@html highlight(`-- Cheap probe (does the provider expose a dashboard for this repo?)
local ok = arbor.security.supports({ repo_id = entry.id })

-- Headline summary used by the dashboard panel.
local sum = arbor.security.summary({
  repo_id    = entry.id,
  range_days = 90,    -- optional, clamped to [7, 90], default 30
})
-- sum.counts          : { critical, high, medium, low, info, unknown }   (active-only)
-- sum.median_age_days : same shape, days as integers (or nil per severity)
-- sum.risk_score      : { value: number, label: "Low|Medium|High|Critical" } | nil
-- sum.time_series     : { points = [...], range_days } | nil
-- sum.web_url         : provider-native dashboard URL

-- Findings list â€” defaults to active scope.
local list = arbor.security.findings({
  repo_id    = entry.id,
  severities = {"critical", "high"},      -- optional
  states     = {"resolved", "dismissed"}, -- optional, default {detected, confirmed}
  search     = "deserialization",
  limit      = 200,
})
-- Each: { id, severity, state, title, description?, scanner?, report_type?,
--         file_path?, start_line?, web_url?, created_at, age_days, identifiers, provider }`, '.lua')}</pre>

<h2>arbor.meta â€” plugin identity &amp; environment</h2>
<pre class="language-lua">{@html highlight(`arbor.meta.plugin_name()              -- "my-plugin"
arbor.meta.api_version()              -- 1  (Arbor plugin API integer)
arbor.meta.app_version()              -- "0.9.0"  (Arbor app semver string)
arbor.meta.plugin_dir()               -- "/path/to/plugins/my-plugin"
arbor.meta.os()                       -- "windows" | "macos" | "linux"
arbor.meta.plugin_loaded("other")     -- true / false (live + enabled check)`, '.lua')}</pre>
<p>
  <code>plugin_loaded(name)</code> is a synchronous check against the host's
  plugin registry â€” use it to branch on whether a sibling plugin is active
  right now without going through the async, fire-and-forget
  <code>arbor.service.call</code> path (which races against startup and can
  silently no-op on host mutex contention).
</p>
<p>Use <code>arbor.meta.os()</code> to build platform-correct commands and paths:</p>
<pre class="language-lua">{@html highlight(`local is_win = arbor.meta.os() == "windows"
local sep    = is_win and "\\\\" or "/"
local ext    = is_win and ".bat" or ".sh"
-- e.g. build the Tomcat catalina script path:
local bin = tomcat_home .. sep .. "bin" .. sep .. "catalina" .. ext`, '.lua')}</pre>

<h2>arbor.timer â€” deferred / recurring execution</h2>
<pre class="language-lua">{@html highlight(`-- Fire once after delay_ms milliseconds
local id = arbor.timer.after(500, function()
  arbor.log.info("fired after 500ms")
end)

-- Fire every interval_ms milliseconds until cancelled
local id2 = arbor.timer.every(5000, function()
  arbor.log.info("tick")
end)

arbor.timer.cancel(id)   -- cancel a timer by its id`, '.lua')}</pre>
<p><strong>Tip:</strong> prefer <code>arbor.scheduler.register</code> (below) for recurring tasks â€” its triggers are richer (cron, fixed_delay, focus gate) and the registrations are shown in the Plugin Manager so users can stop/start each one individually.</p>

<h2>arbor.scheduler â€” Spring-style background schedules</h2>
<p>
  Opt the plugin into the scheduler with <code>[scheduler] enabled = true</code>
  in <code>plugin.toml</code>, then declare every concrete schedule from
  <code>main.lua</code>. Triggers are modelled on Spring's
  <code>@Scheduled</code> annotation: pick exactly one of <code>fixed_rate</code>,
  <code>fixed_delay</code>, or <code>cron</code>.
</p>
<table class="shortcuts-table">
  <thead><tr><th>Field</th><th>Meaning</th></tr></thead>
  <tbody>
    <tr><td><code>action</code></td><td>String â€” required. Plugin action name fired each tick (subscribe with <code>arbor.events.on(action, fn)</code>).</td></tr>
    <tr><td><code>fixed_rate</code></td><td>Duration. Fire every N regardless of how long the previous handler took. Next fire = previous start + N.</td></tr>
    <tr><td><code>fixed_delay</code></td><td>Duration. Wait N <em>after</em> the previous handler returned. Next fire = previous end + N. Use this when overlap would be harmful.</td></tr>
    <tr><td><code>cron</code></td><td>6-field Spring cron â€” <code>second minute hour day-of-month month day-of-week</code>. Anchored to the wall clock, not to "now + N".</td></tr>
    <tr><td><code>initial_delay</code></td><td>Optional duration. Wait this long before the first fire (fixed_rate / fixed_delay only â€” cron always uses the next matching instant).</td></tr>
    <tr><td><code>on_load</code></td><td>Optional bool. Also fire once immediately at plugin load, in addition to the normal cadence. Default <code>false</code>.</td></tr>
    <tr><td><code>only_when_focused</code></td><td>Optional bool. Skip firing while the app window is unfocused or minimised. The clock keeps ticking; a missed tick is simply dropped. Default <code>false</code>.</td></tr>
  </tbody>
</table>
<p>
  Durations accept bare numbers (seconds), suffix form (<code>"30s"</code>,
  <code>"5m"</code>, <code>"2h"</code>, <code>"1d"</code>), or ISO-8601
  (<code>"PT30S"</code>, <code>"PT1H30M"</code>).
</p>
<pre class="language-lua">{@html highlight(`-- plugin.toml:
--   [scheduler]
--   enabled = true

-- Fixed-rate: every 5 minutes, regardless of handler duration.
arbor.scheduler.register({
  action     = "my_plugin:refresh",
  fixed_rate = "5m",
  on_load    = true,                -- also fire once at plugin load
})

-- Fixed-delay: 30 s AFTER the previous fetch finishes â€” prevents overlap
-- when the network is slow.
arbor.scheduler.register({
  action            = "my_plugin:slow_poll",
  fixed_delay       = "30s",
  initial_delay     = "10s",
  only_when_focused = true,
})

-- Cron: every weekday at 09:30 (sec min hr dom mon dow). Anchored to wall clock.
arbor.scheduler.register({
  action = "my_plugin:morning_brief",
  cron   = "0 30 9 * * MON-FRI",
})

arbor.events.on("my_plugin:refresh", function(_ctx)
  arbor.log.info("tick")
end)`, '.lua')}</pre>
<p>
  Re-calling <code>register</code> with the same <code>action</code> replaces
  the previous entry â€” handy for plugins that recompute cadence from settings.
  Inspect the current set with <code>arbor.scheduler.list()</code>; users can
  also stop/start individual entries from the Plugin Manager.
</p>

<h2>Built-in utility modules</h2>
<p>These are available via <code>require()</code> inside any plugin without adding files â€” they are pre-loaded by the sandbox.</p>
<table class="shortcuts-table">
  <thead><tr><th>Module</th><th>Key exports</th></tr></thead>
  <tbody>
    <tr><td><code>arbor.schema</code></td><td><code>validate(data, rules)</code> â†’ ok, errors Â· <code>check(data, rules)</code> â†’ bool (shows toast on first error)</td></tr>
    <tr><td><code>arbor.async</code></td><td><code>Promise</code> Â· <code>run(fn)</code> Â· <code>await(p)</code> Â· <code>debounce(fn, delay_ms)</code> Â· <code>throttle(fn, interval_ms)</code></td></tr>
    <tr><td><code>arbor.event</code></td><td><code>on(event, fn)</code> Â· <code>off(event, fn?)</code> Â· <code>emit(event, payload)</code> â€” in-process pub/sub between plugin modules</td></tr>
  </tbody>
</table>
<pre class="language-lua">{@html highlight(`-- arbor.schema â€” validate form submissions
local schema = require("arbor.schema")
arbor.events.on("my_plugin:save", function(ctx)
  if not schema.check(ctx, {
    name    = { required = true, max_len = 64 },
    url     = { required = true, pattern = "^https?://" },
    timeout = { min = 1, max = 300 },
  }) then return end   -- check() shows toast on first error
  -- ... proceed with save ...
end)

-- arbor.async â€” promises + debounce
local async   = require("arbor.async")
local refresh = async.debounce(function()
  -- called at most once per 200ms after the last trigger
end, 200)

-- Promise: producers (service.call, job.spawn, ui.confirm) return one.
arbor.service.call("compile-action.resolve_java_home", {})
  :ok(function(r)  arbor.log.info("JAVA_HOME = " .. (r.java_home or "")) end)
  :err(function(e) arbor.log.warn("svc " .. e.kind .. ": " .. e.message) end)

-- Sequential await inside async.run â€” yields the coroutine until each promise settles.
async.run(function()
  local ok, err = arbor.async.await(arbor.ui.confirm{ message = "Proceed?" })
  if err or not ok then return end
  local r, sErr = arbor.async.await(arbor.service.call("greeter.greet", { name = "you" }))
  if sErr then arbor.log.warn(sErr.message); return end
  arbor.log.info(r)
end)

-- arbor.event â€” decouple modules
local ev = require("arbor.event")
ev.on("config_changed", function(payload)
  -- payload.repo, etc.
end)
ev.emit("config_changed", { repo = arbor.repo.current() })`, '.lua')}</pre>
