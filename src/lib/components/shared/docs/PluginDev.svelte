<script lang="ts">
  import { highlight } from '$lib/utils/diff-formatter';
</script>

<h1>Plugin Development</h1>
<p>Arbor supports Lua 5.4 plugins via the <code>mlua</code> crate. Plugins live in a <code>plugins/&lt;name&gt;/</code> directory next to the executable. Each plugin has a <code>plugin.toml</code> manifest and one or more Lua files.</p>

<h2>Directory layout</h2>
<pre><code>plugins/
  my-plugin/
    plugin.toml       â† manifest (required)
    main.lua          â† entry point (default; override with entry = "â€¦")
    doc.html          â† optional: HTML docs shown in this panel under Plugins
    lib/utils.lua     â† require("lib.utils") works inside the plugin sandbox
    config/
      global.lua      â† optional sub-modules</code></pre>

<h2>plugin.toml</h2>
<pre class="language-toml">{@html highlight(`[plugin]
name        = "my-plugin"
version     = "0.1.0"
description = "What it does"
author      = "You"
license     = "MIT"
repository        = "https://github.com/you/my-plugin"
keywords          = ["git", "tool"]
min_arbor_version = "0.1.0"  # optional; rejects plugin on older builds (semver)
arbor_api         = 1        # minimum Arbor plugin API version required
os                = []       # ["windows", "linux", "macos"] â€” empty = cross-platform
entry             = "main.lua" # default; can be changed
doc_file          = "doc.html" # optional: HTML file shown in the Docs panel

[permissions]
network              = []          # allowed hostnames for arbor.http.get
fs                   = "none"      # none | read | write
fs_scope             = []          # [] = sandboxed to the active repo; ["*"] = unrestricted; otherwise extra allowed paths
git                  = "none"      # none | read | write | history_rewrite
issues               = "none"      # none | read | write
toolchain            = "none"      # none | read | write
terminal             = "none"      # none | commands | any
terminal_scope       = []          # allowed command basenames when terminal = "commands"
# env_read accepts: true (all vars) | false (no os.getenv) | allowlist of names
env_read             = ["PATH", "JAVA_HOME"]

[hooks]
on_plugin_load   = true   # fires once after main.lua executes (init/constructor)
on_plugin_unload = true   # fires when Arbor shuts down (cleanup)
on_repo_open     = true   # fires when a repo tab becomes active
on_repo_close  = true   # fires when a repo tab is closed
on_repo_init   = true   # fires when a new repo is initialized from a non-git folder
on_tab_switch  = true   # fires on every tab switch
on_commit        = true
on_push          = true
on_pull          = true
on_checkout      = true
on_fetch         = true
on_branch_create = true
on_branch_delete = true
on_branch_rename = true
on_tag_create    = true
on_tag_delete    = true
on_stash_push    = true
on_stash_pop     = true
on_rebase_start  = true
on_rebase_abort  = true

# Background scheduler â€” opt-in only. Schedule data (action, trigger,
# focus gating, â€¦) is declared from main.lua via arbor.scheduler.register.
[scheduler]
enabled = true

# Settings UI is no longer declared in plugin.toml. Plugins register a
# panel at runtime via \`arbor.ui.settings.panel(...)\` â€” see Plugin
# Development â†’ API: UI for the contribution-based settings model.`, 'toml')}</pre>

<h2>Plugin documentation (doc.html)</h2>
<p>
  Set <code>doc_file = "doc.html"</code> in your manifest to expose plugin-specific
  documentation in this panel. The file is read at runtime and injected as raw HTML under
  the <strong>Plugins</strong> group in the left nav.
</p>
<p>
  Write plain HTML using the same element types the rest of the docs use â€” they inherit
  all styles automatically:
</p>
<table class="shortcuts-table">
  <thead><tr><th>HTML element</th><th>Renders as</th></tr></thead>
  <tbody>
    <tr><td><code>&lt;h1&gt;</code></td><td>Section title (large, with bottom border)</td></tr>
    <tr><td><code>&lt;h2&gt;</code></td><td>Sub-heading (small caps, accent)</td></tr>
    <tr><td><code>&lt;h3&gt;</code></td><td>Tertiary heading</td></tr>
    <tr><td><code>&lt;p&gt;</code></td><td>Body paragraph</td></tr>
    <tr><td><code>&lt;ul&gt;</code> / <code>&lt;ol&gt;</code></td><td>Bulleted / numbered list</td></tr>
    <tr><td><code>&lt;strong&gt;</code></td><td>Bold, primary text colour</td></tr>
    <tr><td><code>&lt;code&gt;</code></td><td>Inline monospace (accent colour)</td></tr>
    <tr><td><code>&lt;pre&gt;&lt;code&gt;</code></td><td>Code block</td></tr>
    <tr><td><code>&lt;kbd&gt;</code></td><td>Keyboard key chip</td></tr>
    <tr><td><code>&lt;table class="shortcuts-table"&gt;</code></td><td>Styled data table</td></tr>
  </tbody>
</table>
<p>
  CSS variables like <code>var(--accent)</code>, <code>var(--text-secondary)</code>,
  <code>var(--bg-overlay)</code> etc. are all available for inline styles if you need
  custom colours.
</p>
<pre><code>&lt;!-- doc.html example --&gt;
&lt;h1&gt;my-plugin&lt;/h1&gt;
&lt;p&gt;Short description of what the plugin does.&lt;/p&gt;

&lt;h2&gt;Getting Started&lt;/h2&gt;
&lt;ol&gt;
  &lt;li&gt;Open a repo â€” the plugin activates automatically.&lt;/li&gt;
  &lt;li&gt;Click &lt;strong&gt;â–¶&lt;/strong&gt; in the Activity Bar to run.&lt;/li&gt;
&lt;/ol&gt;

&lt;h2&gt;Permissions&lt;/h2&gt;
&lt;table class="shortcuts-table"&gt;
  &lt;thead&gt;&lt;tr&gt;&lt;th&gt;Permission&lt;/th&gt;&lt;th&gt;Why&lt;/th&gt;&lt;/tr&gt;&lt;/thead&gt;
  &lt;tbody&gt;
    &lt;tr&gt;&lt;td&gt;&lt;code&gt;fs = "read"&lt;/code&gt;&lt;/td&gt;&lt;td&gt;Reads config files in repo.&lt;/td&gt;&lt;/tr&gt;
  &lt;/tbody&gt;
&lt;/table&gt;</code></pre>

<h2>main.lua skeleton</h2>
<pre class="language-lua">{@html highlight(`-- main.lua â€” thin wiring file
-- Register UI elements, subscribe to hooks. Keep logic in sub-modules.

local state = require("state")        -- sub-module inside this plugin dir

-- â”€â”€ Lifecycle â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€
-- on_plugin_load fires once AFTER main.lua finishes executing.
-- Ideal for one-time initialisation (load settings, register combos, etc.)
arbor.events.on("on_plugin_load", function(ctx)
  arbor.log.info("loaded â€” api_version=" .. ctx.api_version)
  state.init()
end)

-- â”€â”€ Hooks â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€
arbor.events.on("on_repo_open", function(ctx)
  state.set_repo(ctx.repo)
  arbor.log.debug("repo_open: " .. ctx.repo)
end)

arbor.events.on("on_commit", function(ctx)
  arbor.notify{ message = "Committed: " .. ctx.message, level = "success" }
end)

arbor.events.on("on_branch_rename", function(ctx)
  -- ctx.tab_id   : string  â€” the repository tab
  -- ctx.old_name : string  â€” previous branch name
  -- ctx.new_name : string  â€” new branch name
  arbor.log.info("Branch renamed: " .. ctx.old_name .. " -> " .. ctx.new_name)
end)

-- â”€â”€ UI registrations â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€
-- Every UI surface is a "contribution point". Push items via the generic
-- arbor.ui.contribute(point, item) â€” sugar APIs (add_context_menu_item,
-- add_sidebar, â€¦) are shortcuts that wrap this same call.
arbor.ui.contribute("arbor:context-menu:commit", {
  id      = "inspect",
  payload = { label = "Inspect", action = "my_plugin:inspect", icon = "Search" },
})`, '.lua')}</pre>

<h2>Hook and level strings</h2>
<p>Hook names and notification levels are passed as plain string literals. The full list is browseable at runtime via <code>arbor.hooks.list()</code> / <code>arbor.hooks.describe(name)</code>. Log levels also have symbolic constants on <code>arbor.log.LEVELS</code> for autocomplete.</p>
<pre class="language-lua">{@html highlight(`-- Hooks: pass the literal name to arbor.events.on
arbor.events.on("on_repo_open",  function(ctx) end)
arbor.events.on("on_commit",     function(ctx) end)
arbor.events.on("on_pipeline_done", function(ctx) end)

-- Notification levels (arbor.notify): "info" | "success" | "warning" | "error"
arbor.notify{ message = "saved",  level = "success" }
arbor.notify{ message = "be careful", level = "warning" }

-- Log levels: bare strings or arbor.log.LEVELS for autocomplete
arbor.log.warn("unexpected state")
local lvl = arbor.log.LEVELS.WARN  -- == "warn"`, '.lua')}</pre>

<h2>arbor.log â€” logging</h2>
<pre class="language-lua">{@html highlight(`arbor.log.debug("detailed trace")
arbor.log.info("something happened")
arbor.log.warn("unexpected state: " .. tostring(val))
arbor.log.error("fatal: " .. err)
-- All messages are prefixed [plugin-name] in the Arbor log`, '.lua')}</pre>

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
local ok = arbor.repo.fetch_active_tab()   -- requires git = "write" (or higher)`, '.lua')}</pre>

<h2>arbor.meta â€” plugin identity &amp; environment</h2>
<pre class="language-lua">{@html highlight(`arbor.meta.plugin_name()  -- "my-plugin"
arbor.meta.api_version()  -- 1  (Arbor plugin API integer)
arbor.meta.app_version()  -- "0.9.0"  (Arbor app semver string)
arbor.meta.plugin_dir()   -- "/path/to/plugins/my-plugin"
arbor.meta.os()           -- "windows" | "macos" | "linux"`, '.lua')}</pre>
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
<p><strong>Tip:</strong> prefer <code>arbor.scheduler.register</code> for recurring tasks â€” Spring-style triggers (<code>fixed_rate</code> / <code>fixed_delay</code> / <code>cron</code>), focus gating, and per-entry start/stop from the Plugin Manager.</p>

<h2>arbor.ui â€” user interface</h2>
<table class="shortcuts-table">
  <thead><tr><th>Function</th><th>Description</th></tr></thead>
  <tbody>
    <tr><td><code>arbor.notify{`{`} message, title?, level?, action? {`}`}</code></td><td>Add a persistent notification to the in-app notification center. <code>level</code>: <code>"info" | "success" | "warning" | "error"</code> (default <code>"info"</code>).</td></tr>
    <tr><td><code>arbor.ui.form(config)</code></td><td>Display an input form modal; submitting fires <code>submit_action</code></td></tr>
    <tr><td><code>arbor.ui.confirm&#123; message, confirm_label?, confirm_variant?, state? &#125;</code></td><td>Confirmation dialog. Returns a Promise resolving to <code>true</code> on confirm or <code>false</code> on cancel.</td></tr>
    <tr><td><code>arbor.ui.add_sidebar(opts)</code></td><td>Register a plugin panel with its own ActivityBar icon. <code>side: "left"|"right"</code> (default "right"), <code>position: "top"|"bottom"</code> (default "top"). Fires <code>panel:open:&lt;id&gt;</code>; respond with <code>set_panel_content</code>.</td></tr>
    <tr><td><code>arbor.ui.set_panel_content(id, body)</code></td><td>Push form-DSL content (<code>&#123;title, nodes, actions?&#125;</code>) into a registered plugin panel.</td></tr>
    <tr><td><code>arbor.ui.add_graph_combo(opts)</code></td><td>Register a split button (run + dropdown). <code>target</code>: "activity_bar" (default) or "repo_actions"</td></tr>
    <tr><td><code>arbor.ui.set_combo_options&#123; id, options, selected? &#125;</code></td><td>Dynamically update a combo's option list. Optional <code>selected</code> adopts a pick if it appears in <code>options</code> (call from <code>on_repo_open</code> to refresh per-repo).</td></tr>
    <tr><td><code>arbor.ui.contribute_patch(point, id, partial)</code></td><td>Shallow-merge <code>partial</code> into the existing payload of a previously-contributed item â€” without re-specifying the full payload.</td></tr>
    <tr><td><code>arbor.ui.add_separator()</code></td><td>Insert a horizontal separator in the activity bar after the last registered item</td></tr>
    <tr><td><code>arbor.ui.add_context_menu_item(opts)</code></td><td>Add item to the commit/branch/file context menu</td></tr>
    <tr><td><code>arbor.ui.add_menu_item(opts)</code></td><td>Add item to the hamburger menu</td></tr>
    <tr><td><code>arbor.ui.open_path(path)</code></td><td>Hand a file/folder to the OS default handler (Explorer / Finder / xdg-open). Great for "Open in file manager" affordances on artefact folders.</td></tr>
    <tr><td><code>arbor.ui.copy_to_clipboard&#123; text, toast? &#125;</code></td><td>Copy <code>text</code> to the system clipboard via the webview; optional <code>toast</code> overrides the success message.</td></tr>
    <tr><td><code>arbor.ui.show_pipeline_run(run_id)</code></td><td>Open the standalone Pipeline Run detail modal (graph + output log) on top of the current view. Deep-link from a plugin modal / sidebar without toggling the bottom Pipelines panel.</td></tr>
  </tbody>
</table>

<h2>arbor.notify â€” persistent notifications</h2>
<p>Adds a notification to the in-app notification center (bell icon in the status bar). Notifications persist until the user explicitly dismisses them. <code>message</code> is required and non-empty; <code>level</code>, <code>title</code>, and <code>action</code> are optional. Invalid input raises a Lua error at the boundary.</p>
<pre class="language-lua">{@html highlight(`-- arbor.notify{ message, title?, level?, action? }
-- level: "info" | "success" | "warning" | "error"  (default "info")

arbor.notify{ title = "Build succeeded", message = "Release build completed", level = "success" }
arbor.notify{ title = "Build failed",    message = "Exited with code 2 â€” see Jobs panel", level = "error" }
arbor.notify{ message = "Config reloaded" }   -- title-less, defaults to "info"

-- Optional click-action button on the notification card:
arbor.notify{
  message = "Worktree drifted",
  level   = "warning",
  action  = { kind = "open-link-manager", label = "View link", link_id = "abc" },
}`, '.lua')}</pre>

<h2>arbor.command â€” command palette entries</h2>
<p>Register items that appear in the Command Palette (<kbd>Ctrl+K</kbd>). Each entry fires the action <code>command:&lt;id&gt;</code> on the plugin when selected.</p>
<pre class="language-lua">{@html highlight(`arbor.command.register({
  id          = "my-action",    -- unique within this plugin
  title       = "My Action",    -- shown in the palette
  description = "Does something useful",  -- subtitle (optional)
  icon        = "Play",         -- Lucide icon name (optional)
  group       = "My Plugin",    -- section label (optional)
})

-- Handle the action:
arbor.events.on("command:my-action", function(_ctx)
  arbor.notify{ message = "Hello from the palette!", level = "success" }
end)

-- Remove the entry at runtime:
arbor.command.unregister("my-action")`, '.lua')}</pre>

<h2>arbor.keybinding â€” plugin keyboard shortcuts</h2>
<p>Register keyboard shortcuts that fire a Lua action when triggered anywhere in the app. Plugin shortcuts are visible under the <strong>Plugins</strong> group in <strong>Settings â†’ Keybindings</strong> (read-only).</p>
<pre class="language-lua">{@html highlight(`-- arbor.keybinding.register(config)
-- config: { key, action, description?, ctrl?, shift?, alt? }
-- Call once during on_plugin_load.

arbor.events.on("on_plugin_load", function(_ctx)
  arbor.keybinding.register({
    key         = "F5",
    action      = "compile:run",   -- fired as a plugin hook
    description = "Run build",
  })

  arbor.keybinding.register({
    key         = "b",
    ctrl        = true,
    shift       = true,
    action      = "my_plugin:open_dashboard",
    description = "Open plugin dashboard",
  })
end)

-- The action fires your registered handler:
arbor.events.on("compile:run", function(ctx)
  arbor.job.spawn({ name = "Build", command = "make", cwd = arbor.repo.current() })
end)`, '.lua')}</pre>
<p><strong>Note:</strong> plugin keybindings take priority over unbound app keys when the shortcut matches. They do <em>not</em> override user-customised app keybindings.</p>

<h2>arbor.job â€” background jobs</h2>
<p>Use <code>arbor.job</code> for long-running or async work. The job runs in a separate OS thread; output is streamed line-by-line to the Jobs panel. Use <code>arbor.terminal.exec()</code> only for short blocking commands.</p>
<table class="shortcuts-table">
  <thead><tr><th>Function</th><th>Description</th></tr></thead>
  <tbody>
    <tr><td><code>arbor.job.spawn(config)</code></td><td>Launch a background job. Returns <code>(JobHandle, nil)</code> on success or <code>(nil, err)</code> on a spawn-side failure. The handle is a Promise (<code>:ok / :err</code>) with extra <code>.id</code> and <code>:cancel()</code>. Config: <code>name</code>, <code>command</code>, <code>cwd?</code>, <code>env?</code>, <code>category?</code> (string â€” groups jobs into collapsible sections in the overlay, e.g. <code>"Builds"</code> / <code>"Services"</code>), <code>on_done_action?</code> (string â€” sugar), <code>on_done?</code> (function â€” sugar)</td></tr>
    <tr><td><code>arbor.job.list()</code></td><td>Returns a Lua table of all job records</td></tr>
    <tr><td><code>arbor.job.cancel(job_id)</code></td><td>Kill a running job (SIGTERM / taskkill /T). No-op if the job has already finished.</td></tr>
  </tbody>
</table>
<pre class="language-lua">{@html highlight(`-- Promise-style: chain :ok / :err on the JobHandle.
local job, err = arbor.job.spawn({
  name    = "npm build",
  command = "npm run build",
  cwd     = arbor.repo.current(),
})
if err then arbor.notify{ message = "Spawn failed: " .. err, level = "error" }; return end

job:ok(function(ctx)  arbor.notify{ message = "Build succeeded âœ“", level = "success" } end)
   :err(function(ctx) arbor.notify{ message = "Build failed (exit " .. (ctx.exit_code or -1) .. ")", level = "error" } end)

-- on_done / on_done_action stay as zucchero â€” they fire alongside the promise.
arbor.job.spawn({
  name           = "Cargo build",
  command        = "cargo build --release",
  cwd            = arbor.repo.current(),
  on_done_action = "my_plugin:build_done",
})
arbor.events.on("my_plugin:build_done", function(ctx)
  arbor.log.info("exit_code=" .. ctx.exit_code)
end)

-- Job sequencing â€” Pattern 1: build then run via :ok chain.
local function launch_service()
  local svc = arbor.job.spawn({ name = "Server", command = "./server", category = "Services" })
  if svc then svc:ok(function(_) arbor.notify{ title = "Server stopped", message = "", level = "info" } end) end
end

local build = arbor.job.spawn({ name = "Build", command = "make release", category = "Builds" })
if build then
  build:ok(function(_) launch_service() end)
       :err(function(ctx) arbor.notify{ title = "Build failed", message = "exit " .. (ctx.exit_code or -1), level = "error" } end)
end

-- Pattern 2 â€” mutual exclusion with a queue
-- (See compile-action plugin for a full implementation)
local active_build = nil
local pending_run  = nil

arbor.events.on("my_plugin:run", function(ctx)
  if active_build then
    pending_run = ctx.value
    arbor.notify{ title = "Queued after build", message = ctx.value .. " will start when build finishes", level = "info" }
    return
  end
  -- ... spawn service immediately
end)`, '.lua')}</pre>

<h2>arbor.pipeline â€” pipelines</h2>
<p>Define and run multi-stage command pipelines. Results appear in the Pipelines panel (Workflow icon in the Activity Bar). No special permissions required.</p>
<table class="shortcuts-table">
  <thead><tr><th>Function</th><th>Description</th></tr></thead>
  <tbody>
    <tr><td><code>arbor.pipeline.define(config)</code></td><td>Register a pipeline. Config: <code>id</code>, <code>name</code>, <code>description?</code>, <code>icon?</code>, <code>stages[]</code> (each with <code>id</code>, <code>name</code>, <code>steps[]</code>)</td></tr>
    <tr><td><code>arbor.pipeline.run&#123; pipeline_id, cwd? &#125;</code></td><td>Start a pipeline run. Returns <code>(run_id, nil)</code> or <code>(nil, err)</code>. Optional <code>cwd</code> overrides the default repo-root working directory</td></tr>
    <tr><td><code>arbor.pipeline.cancel(run_id)</code></td><td>Cancel a running pipeline (stops after the current step)</td></tr>
    <tr><td><code>arbor.pipeline.list()</code></td><td>Return all pipeline definitions registered by this plugin</td></tr>
  </tbody>
</table>

<h2>arbor.issues â€” Linear issue tracker</h2>
<p>Provides synchronous Lua wrappers around the Linear API. Requires <code>issues = "read"</code> or <code>issues = "write"</code> in <code>[permissions]</code>. See the <strong>Issues (Linear)</strong> section of this documentation for full details, filter options, and hook examples.</p>
<table class="shortcuts-table">
  <thead><tr><th>Function</th><th>Permission</th><th>Description</th></tr></thead>
  <tbody>
    <tr><td><code>arbor.issues.search(filters?)</code></td><td><code>issues = "read"</code></td><td>Search issues. Returns array of issue tables. All filter fields are optional.</td></tr>
    <tr><td><code>arbor.issues.get(id)</code></td><td><code>issues = "read"</code></td><td>Fetch a single issue by UUID. Includes full comment list.</td></tr>
    <tr><td><code>arbor.issues.transition(id, status_id)</code></td><td><code>issues = "write"</code></td><td>Move an issue to a new workflow state. Returns updated issue.</td></tr>
    <tr><td><code>arbor.issues.comment(issue_id, body)</code></td><td><code>issues = "write"</code></td><td>Add a comment. Returns the new comment table.</td></tr>
    <tr><td><code>arbor.issues.branch_name(issue)</code></td><td>â€”</td><td>Pure-computation helper: generates a git branch slug from an issue table.</td></tr>
  </tbody>
</table>

<h2>arbor.terminal.exec â€” blocking shell</h2>
<pre class="language-lua">{@html highlight(`-- Requires terminal permission. Always blocking â€” use arbor.job.spawn for async.
local r, err = arbor.terminal.exec{ command = "git status --short", cwd = arbor.repo.current() }
if err then
  arbor.log.error("exec failed: " .. err)
  return
end
-- r.exit_code : number
-- r.stdout    : string
-- r.stderr    : string`, '.lua')}</pre>

<h2>Plugin Settings UI</h2>
<p>
  Settings are contribution-driven. The modal is an IntelliJ-style two-pane
  layout (sidebar + content) and lets <em>any</em> plugin add sidebar
  entries, drop cards into existing entries, or refresh sections on open.
  See <strong>Plugin Development â†’ API: UI</strong> for the full guide.
</p>
<pre class="language-lua">{@html highlight(`-- in main.lua, at PLUGIN_LOAD:
arbor.ui.settings.panel({
  id    = "main",
  title = "My Plugin â€” Settings",
  on_save = "my_plugin:save_all",
})

arbor.ui.contribute("my-plugin:settings:category", {
  id = "general",
  payload = { label = "General", icon = "Settings" },
})

arbor.ui.contribute("my-plugin:settings:section", {
  id = "general-core",
  payload = {
    category = "general",
    label    = "Core",
    nodes = {
      { type = "text",   name = "api_key", label = "API Key" },
      { type = "select", name = "mode",    label = "Mode",
        options = { { value="fast", label="Fast" }, { value="balanced", label="Balanced" } }},
    },
    on_save = "my_plugin:save_general",
  },
})

arbor.events.on("my_plugin:save_general", function(ctx)
  -- ctx receives the un-prefixed slice for THIS section
  arbor.settings.global.set("api_key", ctx.api_key)
  arbor.settings.global.set("mode",    ctx.mode)
  arbor.notify{ message = "Saved", level = "success" }
end)`, '.lua')}</pre>
<p>The Plugin Manager also exposes a <strong>Clear cache</strong> button (two-click confirmation) that wipes a plugin's <code>global.json</code>.</p>

<h2>Form node types</h2>
<table class="shortcuts-table">
  <thead><tr><th>type</th><th>Key fields</th><th>Notes</th></tr></thead>
  <tbody>
    <tr><td><code>text</code></td><td>name, label, placeholder, default, pattern, pattern_hint, readonly</td><td>Also: password, email, url</td></tr>
    <tr><td><code>textarea</code></td><td>name, label, placeholder, default, rows</td><td></td></tr>
    <tr><td><code>number</code></td><td>name, label, default, min, max, step</td><td></td></tr>
    <tr><td><code>range</code></td><td>name, label, default, min, max, step, show_value, value_format</td><td>value_format: "&#123;v&#125;ms"</td></tr>
    <tr><td><code>checkbox</code></td><td>name, label, default</td><td></td></tr>
    <tr><td><code>select</code></td><td>name, label, default, options[]</td><td>options: value+label+disabled?</td></tr>
    <tr><td><code>radio</code></td><td>name, label, default, options[], inline</td><td>options: value+label+description?</td></tr>
    <tr><td><code>color</code></td><td>name, label, default (#rrggbb)</td><td></td></tr>
    <tr><td><code>kv_list</code></td><td>name, label, key_placeholder, value_placeholder, default</td><td>Submitted as JSON object</td></tr>
    <tr><td><code>section</code></td><td>title, description, children[], collapsible, collapsed, card, count, add_action</td><td><code>card = true</code> â†’ dark title bar + counter pill + optional + button</td></tr>
    <tr><td><code>container</code></td><td>children[], columns, gap</td><td>CSS grid</td></tr>
    <tr><td><code>row</code></td><td>children[], gap, align, wrap</td><td>Flexbox row</td></tr>
    <tr><td><code>separator</code></td><td>label?</td><td>Labelled divider line</td></tr>
    <tr><td><code>divider</code></td><td>â€”</td><td>Plain &lt;hr&gt;</td></tr>
    <tr><td><code>paragraph</code></td><td>content, variant (normal/muted/heading/caption)</td><td></td></tr>
    <tr><td><code>label</code></td><td>text, variant</td><td>Static text alias</td></tr>
    <tr><td><code>alert</code></td><td>text, variant (info/warning/error/success)</td><td></td></tr>
    <tr><td><code>code</code></td><td>text, language?</td><td>Monospace block</td></tr>
    <tr><td><code>button</code></td><td>label?, action, variant, close_after, disabled, icon, icon_only, tooltip, extra</td><td>Inline action; <code>icon_only</code> for toolbar look</td></tr>
    <tr><td><code>menu_button</code></td><td>label?, icon, icon_only, tooltip, options[]</td><td>Button that opens a dropdown menu</td></tr>
    <tr><td><code>tree_layout</code></td><td>nav_children[], content_children[], nav_width</td><td>2-col split (IntelliJ-style run configs)</td></tr>
    <tr><td><code>card_row</code></td><td>label, description, children[]</td><td>Two-column row inside a card section</td></tr>
    <tr><td><code>cfg_list</code></td><td>items[]</td><td>Item rows with active dot + edit/delete on hover</td></tr>
    <tr><td><code>suggest_grid</code></td><td>items[]</td><td>2-col grid of suggestion cards</td></tr>
    <tr><td><code>counter_grid</code></td><td>items[], min_width?, gap?, padding?, actions.select?</td><td>Responsive KPI tile grid; <code>select</code> fires <code>&#123; key &#125;</code></td></tr>
    <tr><td><code>score_gauge</code></td><td>value, min, max, segments[], label, size, value_color</td><td>Semi-circle gauge; display only</td></tr>
    <tr><td><code>time_series_chart</code></td><td>series[], x_kind, height, show_legend, y_include_zero</td><td>Multi-series line chart with hover tooltip + legend</td></tr>
    <tr><td><code>data_table</code></td><td>columns[], rows[], row_key?, height?, initial_sort?, empty?, actions.row_click?</td><td>Sortable / clickable table; <code>row_click</code> fires <code>&#123; row_id, row &#125;</code></td></tr>
  </tbody>
</table>
<p>
  Field nodes (inputs) are described in detail on the <strong>API â€” UI</strong>
  page along with the full form DSL (including <code>tree_layout</code>,
  <code>menu_button</code>, <code>arbor.ui.form.replace</code>). Any node supports
  <code>show_if</code> for conditional visibility â€” operators: <code>eq</code>,
  <code>neq</code>, <code>gt</code>, <code>lt</code>, <code>gte</code>, <code>lte</code>,
  <code>in</code>/<code>in_values</code>, <code>nin</code>, and logical
  <code>and</code>/<code>or</code>/<code>not</code>.
</p>

<h2>Form state â€” opaque context echo</h2>
<p>Pass a <code>state</code> table to <code>form</code> to carry server-side context that isn't rendered in the UI but is echoed back unchanged in every <code>ctx</code> payload (submit, button actions, cancel).</p>
<pre class="language-lua">{@html highlight(`arbor.ui.form({
  title         = "Edit Config",
  submit_action = "my_plugin:update",
  state         = { config_id = "cfg-42", revision = 3 },
  nodes = {
    { type = "text", name = "label", label = "Label" },
  },
})

arbor.events.on("my_plugin:update", function(ctx)
  -- ctx.label       = user input
  -- ctx.state.config_id = "cfg-42"   (echoed unchanged)
  -- ctx.state.revision  = 3
end)`, '.lua')}</pre>

<h2>Combo Button</h2>
<p>
  A split widget: a primary action button (icon only) on the left and a dropdown arrow on the right.
  <code>run_icon</code> accepts any Lucide icon name â€” common choices: <code>"Play"</code> (â–¶),
  <code>"Hammer"</code> (ðŸ”¨), <code>"Wrench"</code>, <code>"Zap"</code>.
  You can register <strong>multiple combos</strong> from the same plugin; they appear in
  registration order within the target area.
</p>
<pre class="language-lua">{@html highlight(`-- Register once (e.g. in on_plugin_load).
-- Example: two combos â€” Run (â–¶) and Build (ðŸ”¨).
arbor.ui.add_graph_combo({
  id         = "my_plugin:run",
  run_icon   = "Play",           -- Lucide icon name
  run_action = "my_plugin:do_run",
  tooltip    = "Run application",
  target     = "repo_actions",   -- or "activity_bar"
  options    = {},
})
arbor.ui.add_graph_combo({
  id         = "my_plugin:build",
  run_icon   = "Hammer",
  run_action = "my_plugin:do_build",
  tooltip    = "Build project",
  target     = "repo_actions",
  options    = {},
})

-- Refresh options when repo changes
arbor.events.on("on_repo_open", function(ctx)
  arbor.ui.set_combo_options{
    id = "my_plugin:run",
    options = {
      { value = "dev",  label = "Run Â· dev",  group = "Project" },
      { value = "prod", label = "Run Â· prod", group = "Project" },
    },
  }
  arbor.ui.set_combo_options{
    id = "my_plugin:build",
    options = {
      { value = "debug",   label = "Build Â· debug",   group = "Project" },
      { value = "release", label = "Build Â· release", group = "Project" },
    },
  }
end)

arbor.events.on("my_plugin:do_run", function(ctx)
  -- ctx.value = selected option value
  arbor.job.spawn({ name = "Run " .. ctx.value, command = "make run_" .. ctx.value,
                    cwd = arbor.repo.current() })
end)

arbor.events.on("my_plugin:do_build", function(ctx)
  arbor.job.spawn({ name = "Build " .. ctx.value, command = "make " .. ctx.value,
                    cwd = arbor.repo.current() })
end)`, '.lua')}</pre>

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

-- arbor.async â€” Promise + debounce
local async  = require("arbor.async")
local refresh = async.debounce(function()
  -- called at most once per 200ms after the last trigger
end, 200)

-- Promise: chain :ok / :err on results returned by service.call / job.spawn / ui.confirm.
arbor.ui.confirm{ message = "Reset workdir?" }
  :ok(function(yes) if yes then arbor.log.info("user confirmed") end end)

-- Sequential await inside async.run â€” yields the coroutine until each promise settles.
async.run(function()
  local r, err = arbor.async.await(arbor.service.call("greeter.greet", { name = "Arbor" }))
  if err then arbor.log.warn(err.message); return end
  arbor.log.info(r)
end)

-- arbor.event â€” decouple modules
local ev = require("arbor.event")
ev.on("config_changed", function(payload)
  -- payload.repo, etc.
end)
ev.emit("config_changed", { repo = arbor.repo.current() })`, '.lua')}</pre>

<h2>require() sandbox</h2>
<p><code>require()</code> inside a plugin is sandboxed to the plugin directory. Dots in the module name are converted to path separators (<code>require("lib.utils")</code> â†’ <code>plugins/my-plugin/lib/utils.lua</code>). Path traversal attempts (<code>../</code>) raise a Lua error. Standard Lua packages (<code>string</code>, <code>table</code>, <code>math</code>, <code>os</code>) are always available.</p>

<h2>Hooks reference</h2>
<p>Declare which hooks your plugin subscribes to via boolean flags in <code>[hooks]</code>. Each hook handler is registered with <code>arbor.events.on("on_hook_name", fn)</code> in Lua.</p>
<table class="shortcuts-table">
  <thead><tr><th>Hook constant</th><th>TOML key</th><th>Context fields</th></tr></thead>
  <tbody>
    <tr><td colspan="3" style="color:var(--text-muted);font-size:0.78rem;padding-top:0.6rem">â”€â”€ Lifecycle â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€</td></tr>
    <tr><td><code>PLUGIN_LOAD</code></td><td><code>on_plugin_load</code></td><td>plugin_name, dir, api_version</td></tr>
    <tr><td><code>REPO_OPEN</code></td><td><code>on_repo_open</code></td><td>tab_id, path, name</td></tr>
    <tr><td><code>REPO_CLOSE</code></td><td><code>on_repo_close</code></td><td>tab_id, path, name</td></tr>
    <tr><td><code>REPO_INIT</code></td><td><code>on_repo_init</code></td><td>path, name, default_branch, provider, remote_url, has_readme, license, gitignore</td></tr>
    <tr><td><code>TAB_SWITCH</code></td><td><code>on_tab_switch</code></td><td>tab_id</td></tr>
    <tr><td colspan="3" style="color:var(--text-muted);font-size:0.78rem;padding-top:0.6rem">â”€â”€ Git operations â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€</td></tr>
    <tr><td><code>COMMIT</code></td><td><code>on_commit</code></td><td>tab_id, oid, message, amend</td></tr>
    <tr><td><code>PUSH</code></td><td><code>on_push</code></td><td>tab_id, remote, refspec, force</td></tr>
    <tr><td><code>PULL</code></td><td><code>on_pull</code></td><td>tab_id, remote</td></tr>
    <tr><td><code>FETCH</code></td><td><code>on_fetch</code></td><td>tab_id, remote</td></tr>
    <tr><td><code>CHECKOUT</code></td><td><code>on_checkout</code></td><td>tab_id, branch <em>or</em> oid (detached)</td></tr>
    <tr><td colspan="3" style="color:var(--text-muted);font-size:0.78rem;padding-top:0.6rem">â”€â”€ Branch / tag â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€</td></tr>
    <tr><td><code>BRANCH_CREATE</code></td><td><code>on_branch_create</code></td><td>tab_id, name, from_oid</td></tr>
    <tr><td><code>BRANCH_DELETE</code></td><td><code>on_branch_delete</code></td><td>tab_id, name <em>or</em> names[] (bulk delete)</td></tr>
    <tr><td><code>BRANCH_RENAME</code></td><td><code>on_branch_rename</code></td><td>tab_id, old_name, new_name</td></tr>
    <tr><td><code>TAG_CREATE</code></td><td><code>on_tag_create</code></td><td>tab_id, name, oid, annotated</td></tr>
    <tr><td><code>TAG_DELETE</code></td><td><code>on_tag_delete</code></td><td>tab_id, name</td></tr>
    <tr><td colspan="3" style="color:var(--text-muted);font-size:0.78rem;padding-top:0.6rem">â”€â”€ Stash â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€</td></tr>
    <tr><td><code>STASH_PUSH</code></td><td><code>on_stash_push</code></td><td>tab_id, index, message, include_untracked</td></tr>
    <tr><td><code>STASH_POP</code></td><td><code>on_stash_pop</code></td><td>tab_id, index, drop (true=pop, false=apply)</td></tr>
    <tr><td colspan="3" style="color:var(--text-muted);font-size:0.78rem;padding-top:0.6rem">â”€â”€ Rebase â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€</td></tr>
    <tr><td><code>REBASE_START</code></td><td><code>on_rebase_start</code></td><td>tab_id, base, action_count</td></tr>
    <tr><td><code>REBASE_ABORT</code></td><td><code>on_rebase_abort</code></td><td>tab_id</td></tr>
    <tr><td colspan="3" style="color:var(--text-muted);font-size:0.78rem;padding-top:0.6rem">â”€â”€ Git Flow â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€</td></tr>
    <tr><td><code>FLOW_INIT</code></td><td><code>on_flow_init</code></td><td>tab_id</td></tr>
    <tr><td><code>FLOW_FEATURE_START</code></td><td><code>on_flow_feature_start</code></td><td>tab_id, name</td></tr>
    <tr><td><code>FLOW_FEATURE_FINISH</code></td><td><code>on_flow_feature_finish</code></td><td>tab_id, name</td></tr>
    <tr><td><code>FLOW_RELEASE_START</code></td><td><code>on_flow_release_start</code></td><td>tab_id, version</td></tr>
    <tr><td><code>FLOW_RELEASE_FINISH</code></td><td><code>on_flow_release_finish</code></td><td>tab_id, version</td></tr>
    <tr><td><code>FLOW_HOTFIX_START</code></td><td><code>on_flow_hotfix_start</code></td><td>tab_id, name</td></tr>
    <tr><td><code>FLOW_HOTFIX_FINISH</code></td><td><code>on_flow_hotfix_finish</code></td><td>tab_id, name</td></tr>
    <tr><td colspan="3" style="color:var(--text-muted);font-size:0.78rem;padding-top:0.6rem">â”€â”€ Pipelines â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€</td></tr>
    <tr><td><code>PIPELINE_STARTED</code></td><td><code>on_pipeline_started</code></td><td>run_id, pipeline_id, plugin</td></tr>
    <tr><td><code>PIPELINE_STEP_DONE</code></td><td><code>on_pipeline_step_done</code></td><td>run_id, stage, step, exit_code</td></tr>
    <tr><td><code>PIPELINE_DONE</code></td><td><code>on_pipeline_done</code></td><td>run_id, plugin, status</td></tr>
    <tr><td colspan="3" style="color:var(--text-muted);font-size:0.78rem;padding-top:0.6rem">â”€â”€ Merge Requests / Pull Requests â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€</td></tr>
    <tr><td><code>MR_OPENED</code></td><td><code>on_mr_opened</code></td><td>number, title, source_branch, target_branch, provider</td></tr>
    <tr><td><code>MR_MERGED</code></td><td><code>on_mr_merged</code></td><td>number, provider</td></tr>
    <tr><td><code>MR_UPDATED</code></td><td><code>on_mr_updated</code></td><td>number, provider</td></tr>
    <tr><td colspan="3" style="color:var(--text-muted);font-size:0.78rem;padding-top:0.6rem">â”€â”€ Issues (Linear) â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€</td></tr>
    <tr><td><code>ISSUE_LINKED</code></td><td><code>on_issue_linked</code></td><td>issue_id, identifier, sha, branch</td></tr>
    <tr><td><code>ISSUE_TRANSITIONED</code></td><td><code>on_issue_transitioned</code></td><td>issue_id, identifier, from_status, to_status</td></tr>
    <tr><td colspan="3" style="color:var(--text-muted);font-size:0.78rem;padding-top:0.6rem">â”€â”€ Theme / branding â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€</td></tr>
    <tr><td><code>THEME_CHANGED</code></td><td><code>on_theme_changed</code></td><td>theme_id, theme_name, vars (merged effective stylesheet), source ("user"|"plugin"|"init")</td></tr>
    <tr><td colspan="3" style="color:var(--text-muted);font-size:0.78rem;padding-top:0.6rem">â”€â”€ Schedulers â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€</td></tr>
    <tr><td>(action name)</td><td><code>arbor.scheduler.register</code></td><td>Spring-style triggers: <code>fixed_rate</code> / <code>fixed_delay</code> / <code>cron</code>. Manifest opt-in: <code>[scheduler] enabled = true</code></td></tr>
  </tbody>
</table>

<h2>Permissions reference</h2>
<ul>
  <li><strong>fs</strong> â€” <code>"none"</code> (default), <code>"read"</code>, or <code>"write"</code>. Higher implies lower</li>
  <li><strong>fs_scope</strong> â€” <code>[]</code> (default) sandboxes <code>arbor.fs.*</code> to the active repo; <code>["*"]</code> grants unrestricted access; otherwise a list of absolute paths to allow in addition to the active repo</li>
  <li><strong>network</strong> â€” list of allowed hostnames; empty = no network</li>
  <li><strong>git</strong> â€” <code>"none"</code> (default), <code>"read"</code> (<code>arbor.repo.*</code> read ops), <code>"write"</code> (commit, branch, fetch/push, notes, clone, stash), or <code>"history_rewrite"</code> (rebase, <code>reset --hard</code>, force-push, amend, filter-branch â€” destructive)</li>
  <li><strong>issues</strong> â€” <code>"none"</code> (default), <code>"read"</code> (<code>arbor.issues.search/get</code>), or <code>"write"</code> (transition / comment; implies read)</li>
  <li><strong>toolchain</strong> â€” <code>"none"</code> (default), <code>"read"</code> (list / active / detect / env), or <code>"write"</code> (add / remove / set_active)</li>
  <li><strong>terminal</strong> â€” <code>"none"</code> (default), <code>"any"</code> (any command), or <code>"commands"</code> (only basenames in <code>terminal_scope</code>)</li>
  <li><strong>env_read</strong> â€” <code>true</code> (default; all vars), <code>false</code> (<code>os.getenv</code> removed), or an allowlist like <code>["PATH", "JAVA_HOME"]</code></li>
  <li><strong>service_call / service_export</strong> â€” booleans; allow <code>arbor.service.call</code> / <code>arbor.service.export</code></li>
  <li><strong>settings_read_others</strong> â€” boolean; allow <code>arbor.settings.read(plugin, key)</code> on other plugins' globals</li>
</ul>

<h2>Manifest top-level fields</h2>
<ul>
  <li><strong>min_arbor_version</strong> <em>(optional)</em> â€” semver string. The plugin is rejected at load time if the running Arbor build is older. Plain strings (<code>"0.5.0"</code>) are interpreted as <code>&gt;=0.5.0</code>; full semver requirements (<code>"&gt;=0.5, &lt;0.7"</code>) are also accepted.</li>
  <li><strong>arbor_api</strong> â€” integer Lua API contract version. Bumped on breaking changes. Plugins requiring a higher version than the build supports are rejected.</li>
  <li><strong>os</strong> <em>(optional)</em> â€” list of supported operating systems (<code>"windows"</code>, <code>"linux"</code>, <code>"macos"</code>). Empty/missing = cross-platform. Plugins are skipped at discovery on non-listed hosts.</li>
</ul>
<p>Dangerous Lua functions (<code>os.execute</code>, <code>os.exit</code>, <code>os.remove</code>, <code>os.rename</code>, <code>io.*</code>, <code>load</code>, <code>loadfile</code>, <code>dofile</code>) are removed from the sandbox. The terminal permission is captured at plugin load time â€” it cannot be escalated by overwriting a Lua global.</p>

<h2>Multi-file plugin layout (recommended)</h2>
<pre><code>plugins/compile-action/
  plugin.toml
  main.lua              â† thin wiring: require sub-modules, register hooks/UI
  state.lua             â† shared mutable state (current repo, running job IDs)
  detect.lua            â† project type auto-detection (Maven/Gradle/npm/â€¦)
  defaults.lua          â† default build configs per project type
  run_defaults.lua      â† default run configs per project type
  config/
    global.lua          â† global build settings CRUD + form
    project.lua         â† per-repo build settings CRUD + form
    run_global.lua      â† global run settings CRUD + form (+ auto_stop global default)
    run_project.lua     â† per-repo run settings CRUD + form (+ tomcat_home, auto_stop override)
    jdk.lua             â† JDK registry (shared by build + run)
  ui/
    combo.lua           â† build combo (Hammer icon)
    run_combo.lua       â† run combo (Play icon)</code></pre>
<pre class="language-lua">{@html highlight(`-- main.lua
local state     = require("state")
local combo     = require("ui.combo")
local run_combo = require("ui.run_combo")

arbor.events.on("on_plugin_load", function(ctx)
  combo.register()      -- ðŸ”¨ Build combo (right)
  run_combo.register()  -- â–¶  Run combo (left)

  arbor.keybinding.register({ key = "F9", action = "compile:run", description = "Build selected" })
  arbor.keybinding.register({ key = "F5", action = "run:run",     description = "Run selected"   })
end)

arbor.events.on("on_repo_open", function(ctx)
  state.set_repo(ctx.path)
  combo.refresh()
  run_combo.refresh()
end)

-- Build action
arbor.events.on("compile:run", function(ctx)
  local cfg = resolve_build_config(ctx.value)
  arbor.job.spawn({ name = cfg.label, command = cfg.command, cwd = cfg.cwd })
end)

-- Run action â€” stops existing instance first (if auto_stop is enabled)
arbor.events.on("run:run", function(ctx)
  local existing = state.get_running(ctx.value)
  if existing and auto_stop_enabled() then
    arbor.job.cancel(existing)
    state.untrack_run(ctx.value)
  end

  local cfg = resolve_run_config(ctx.value)
  local job, spawn_err = arbor.job.spawn({
    name    = cfg.label,
    command = cfg.command,
    cwd     = cfg.cwd,
    on_done = function(_) state.untrack_run(ctx.value) end,
  })
  if not job then arbor.log.warn("spawn failed: " .. spawn_err); return end
  state.track_run(ctx.value, job.id)
end)`, '.lua')}</pre>
