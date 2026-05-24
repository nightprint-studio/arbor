<script lang="ts">
  import { highlight } from '$lib/utils/diff-formatter';
  import Callout from '$lib/components/shared/ui/Callout.svelte';
</script>

<h1>Plugin Development — Basics</h1>

<p class="doc-lead">Arbor embeds <strong>Lua 5.4</strong> via the <code>mlua</code> crate. Plugins live in <code>plugins/&lt;name&gt;/</code> next to the executable and need only a <code>plugin.toml</code> manifest plus an entry-point Lua file.</p>

<dl class="meta-grid">
  <dt>Runtime</dt><dd>Lua 5.4 (vendored) — no system Lua needed</dd>
  <dt>Manifest</dt><dd><code>plugin.toml</code> — required</dd>
  <dt>Entry point</dt><dd><code>main.lua</code> by default; override with <code>entry</code></dd>
  <dt>API version</dt><dd>Declare minimum required via <code>arbor_api</code></dd>
  <dt>Sandbox</dt><dd><code>require()</code> scoped to the plugin dir; dangerous stdlib removed</dd>
</dl>

<h2>Directory layout</h2>
<pre><code>plugins/
  my-plugin/
    plugin.toml       ← manifest (required)
    main.lua          ← entry point (default; override with entry = "…")
    doc.html          ← optional: HTML docs shown in this panel under Plugins
    lib/utils.lua     ← require("lib.utils") works inside the plugin sandbox
    config/
      global.lua      ← optional sub-modules</code></pre>

<h2>Installing &amp; sharing plugins</h2>
<p>Open the <strong>Plugin Manager</strong> (Activity Bar → puzzle icon) — the top-right toolbar exposes two shortcuts that avoid hand-editing files on disk:</p>
<div class="feature-grid two-col">
  <div class="feature-card">
    <div class="fc-title">Import from <code>.zip</code></div>
    <div class="fc-desc">
      The <strong>Upload</strong> icon opens a file picker; pick a plugin archive and Arbor extracts it into <code>plugins/&lt;name&gt;/</code>, then reloads. The zip must contain a top-level <code>plugin.toml</code> (either at the archive root or inside a single wrapping folder). Existing folders with the same name are refused — delete the old copy first. Imported plugins land <strong>disabled by default</strong> — review the manifest's <code>[permissions]</code>, then click the <strong>Power</strong> icon on the plugin card to enable.
    </div>
  </div>
  <div class="feature-card">
    <div class="fc-title">Export Template wizard</div>
    <div class="fc-desc">
      The <strong>Wand</strong> icon opens a 4-step wizard (Identity → Permissions → Hooks → Recipes) that scaffolds a starter plugin and saves it as a zip. Each toggled recipe (command palette entry, keybinding, settings panel, modal form, toolbar action, sidebar, notification, background job, scheduler, HTTP) injects a canonical Lua snippet into <code>main.lua</code>. The bundle ships with <code>sdk.d.lua</code> + <code>.luarc.json</code> so lua-language-server provides <code>arbor.*</code> autocomplete in any editor.
    </div>
  </div>
</div>
<div class="hint">Once exported, unzip into <code>plugins/&lt;name&gt;/</code> and click <strong>Reload</strong> in the Plugin Manager — or hand the zip to another user and have them <em>Import</em> it.</div>

<h2>Managing installed plugins</h2>
<p>The Plugin Manager lists every plugin discovered in <code>plugins/</code>. Each row exposes a fixed action column on the right (left → right):</p>
<table>
  <thead><tr><th>Icon</th><th>Action</th><th>Behaviour</th></tr></thead>
  <tbody>
    <tr><td>⚙️ <strong>Settings</strong></td><td>Open the plugin's settings panel</td><td>Visible only when the plugin registered a settings container via <code>arbor.ui.settings.panel(...)</code>. Disabled while the plugin is off.</td></tr>
    <tr><td>ℹ️ <strong>Info</strong></td><td>Open the <em>Plugin Info</em> modal</td><td>Detailed read-out of identity, permissions, hooks, schedulers + maintenance actions (see below).</td></tr>
    <tr><td>🗑 <strong>Uninstall</strong></td><td>Permanently remove the plugin</td><td>Deletes the <code>plugins/&lt;name&gt;/</code> folder, the global <code>plugin_data/&lt;name&gt;/</code> store, every per-repo <code>.arbor/plugins/&lt;name&gt;/</code>, and the persisted enable-state. Shows a cascade-warning modal if other enabled plugins still depend on it.</td></tr>
    <tr><td>⏻ <strong>Power</strong></td><td>Enable / Disable</td><td>Persisted across restarts. Disabling stops every scheduler, fires <code>on_plugin_unload</code>, and closes any sidebar / panel that the plugin owned. Re-enabling reloads the plugin and re-fires <code>on_plugin_load</code>.</td></tr>
  </tbody>
</table>

<h3>Master kill-switch</h3>
<p>The toggle at the top of the modal — <em>Abilita gestione plugin</em> — is the <strong>master kill-switch</strong>. While it is off, the runtime is empty: nothing is loaded at startup, no schedulers fire, the contribution registry is wiped, and the per-plugin list is hidden until the switch is flipped back on. Useful for diagnosing whether a misbehaving plugin is the cause of an issue.</p>

<h3>Plugin Info modal</h3>
<p>Opened from the <strong>Info</strong> icon. Six grouped sections:</p>
<div class="feature-grid two-col">
  <div class="feature-card">
    <div class="fc-title">Identity</div>
    <div class="fc-desc">Name, version, author, license, declared <code>arbor_api</code>, repository link (clickable, opens in the system browser) and keyword chips.</div>
  </div>
  <div class="feature-card">
    <div class="fc-title">Schedulers</div>
    <div class="fc-desc">One row per <code>arbor.scheduler.register</code> call with the action name, trigger summary (<code>every 5m</code>, <code>cron(…)</code>, …) and a per-action <strong>toggle</strong>. The header row exposes <em>Enable all</em> / <em>Disable all</em> bulk buttons. Toggling a single schedule calls <code>start_plugin_scheduler</code> / <code>stop_plugin_scheduler</code> on the backend; the change is in-memory only — restarting Arbor re-applies the manifest defaults.</div>
  </div>
  <div class="feature-card">
    <div class="fc-title">Permissions</div>
    <div class="fc-desc">Coloured pills (<span class="badge">safe</span> / <span class="badge badge-opt">warn</span> / danger) for filesystem scope, network allow-list, git capability tier and terminal access — same chips shown when reviewing imported plugins.</div>
  </div>
  <div class="feature-card">
    <div class="fc-title">Hooks</div>
    <div class="fc-desc">Lists every <code>[hooks]</code> entry the manifest opted into so reviewers can see at a glance which lifecycle events the plugin observes.</div>
  </div>
  <div class="feature-card">
    <div class="fc-title">Maintenance · Open settings</div>
    <div class="fc-desc">Shortcut to the plugin's settings container without first closing the modal. Disabled when the plugin is off or hasn't registered one.</div>
  </div>
  <div class="feature-card">
    <div class="fc-title">Maintenance · Clear settings cache</div>
    <div class="fc-desc">Two-step destructive button (first click arms it red, second click confirms). Wipes every persisted setting written by this plugin (global + per-repo) — the plugin's own folder and code stay untouched. Use after schema breaking changes or to reset a misbehaving config.</div>
  </div>
</div>
<div class="hint">The Info modal stays in sync with backend events — reloading plugins or toggling the master switch refreshes its content automatically.</div>

<h2>plugin.toml</h2>
<pre class="language-toml">{@html highlight(`[plugin]
name        = "my-plugin"
version     = "0.1.0"
description = "What it does"
author      = "You"
license     = "MIT"
repository  = "https://github.com/you/my-plugin"
keywords          = ["git", "tool"]
min_arbor_version = "0.1.0"  # optional; rejects plugin on older builds (semver)
arbor_api         = 1        # minimum Arbor plugin API version required
os                = []       # ["windows", "linux", "macos"] — empty = cross-platform
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
# service_call         = false    # arbor.service.call — invoke services from other plugins
# service_export       = false    # arbor.service.export — expose callable services
# settings_read_others = false    # arbor.settings.read other plugins' globals

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

# Background scheduler — opt-in only. Schedule data (action, trigger,
# focus gating, …) is declared from main.lua via arbor.scheduler.register.
[scheduler]
enabled = true

# Settings UI is no longer declared in plugin.toml. Plugins register a
# panel at runtime via \`arbor.ui.settings.panel(...)\` — see Plugin
# Development → API: UI for the contribution-based settings model.`, 'toml')}</pre>

<h2>Plugin documentation (doc.html)</h2>
<p>Set <code>doc_file = "doc.html"</code> in your manifest to expose plugin-specific documentation under the <strong>Plugins</strong> group in the left nav. Plain HTML — styles from the host docs apply automatically.</p>

<h3>Supported elements</h3>
<table>
  <thead><tr><th>Tag</th><th>Renders as</th></tr></thead>
  <tbody>
    <tr><td><code>&lt;h1&gt;</code></td><td>Section title (large, bottom border)</td></tr>
    <tr><td><code>&lt;h2&gt;</code></td><td>Sub-heading (small caps, accent)</td></tr>
    <tr><td><code>&lt;h3&gt;</code> / <code>&lt;h4&gt;</code></td><td>Tertiary / quaternary heading</td></tr>
    <tr><td><code>&lt;p&gt;</code> · <code>&lt;ul&gt;</code> · <code>&lt;ol&gt;</code></td><td>Body text and lists</td></tr>
    <tr><td><code>&lt;strong&gt;</code></td><td>Bold, primary text colour</td></tr>
    <tr><td><code>&lt;code&gt;</code> · <code>&lt;pre&gt;&lt;code&gt;</code></td><td>Inline / block monospace</td></tr>
    <tr><td><code>&lt;kbd&gt;</code></td><td>Keyboard key chip</td></tr>
    <tr><td><code>&lt;table&gt;</code></td><td>Styled data table</td></tr>
  </tbody>
</table>

<div class="hint">CSS variables like <code>var(--accent)</code>, <code>var(--text-secondary)</code>, <code>var(--bg-overlay)</code> are available for custom inline styling.</div>
<pre><code>&lt;!-- doc.html example --&gt;
&lt;h1&gt;my-plugin&lt;/h1&gt;
&lt;p&gt;Short description of what the plugin does.&lt;/p&gt;

&lt;h2&gt;Getting Started&lt;/h2&gt;
&lt;ol&gt;
  &lt;li&gt;Open a repo — the plugin activates automatically.&lt;/li&gt;
  &lt;li&gt;Click &lt;strong&gt;▶&lt;/strong&gt; in the Activity Bar to run.&lt;/li&gt;
&lt;/ol&gt;

&lt;h2&gt;Permissions&lt;/h2&gt;
&lt;table class="shortcuts-table"&gt;
  &lt;thead&gt;&lt;tr&gt;&lt;th&gt;Permission&lt;/th&gt;&lt;th&gt;Why&lt;/th&gt;&lt;/tr&gt;&lt;/thead&gt;
  &lt;tbody&gt;
    &lt;tr&gt;&lt;td&gt;&lt;code&gt;fs = "read"&lt;/code&gt;&lt;/td&gt;&lt;td&gt;Reads config files in repo.&lt;/td&gt;&lt;/tr&gt;
  &lt;/tbody&gt;
&lt;/table&gt;</code></pre>

<h2>main.lua skeleton</h2>
<pre class="language-lua">{@html highlight(`-- main.lua — thin wiring file
-- Register UI elements, subscribe to hooks. Keep logic in sub-modules.

local state = require("state")        -- sub-module inside this plugin dir

-- ── Lifecycle ──────────────────────────────────────────────────────────────────
-- on_plugin_load fires once AFTER main.lua finishes executing.
-- Ideal for one-time initialisation (load settings, register combos, etc.)
arbor.events.on("on_plugin_load", function(ctx)
  arbor.log.info("loaded — api_version=" .. ctx.api_version)
  state.init()
end)

-- ── Hooks ──────────────────────────────────────────────────────────────────────
arbor.events.on("on_repo_open", function(ctx)
  state.set_repo(ctx.repo)
  arbor.log.debug("repo_open: " .. ctx.repo)
end)

arbor.events.on("on_commit", function(ctx)
  arbor.notify{ message = "Committed: " .. ctx.message, level = "success" }
end)

arbor.events.on("on_branch_rename", function(ctx)
  -- ctx.tab_id   : string  — the repository tab
  -- ctx.old_name : string  — previous branch name
  -- ctx.new_name : string  — new branch name
  arbor.log.info("Branch renamed: " .. ctx.old_name .. " -> " .. ctx.new_name)
end)

-- ── UI registrations ───────────────────────────────────────────────────────────
arbor.ui.add_context_menu_item({ target = "commit", label = "Inspect", action = "my_plugin:inspect", icon = "Search" })`, '.lua')}</pre>

<h2>require() sandbox</h2>
<p><code>require()</code> inside a plugin is sandboxed to the plugin directory. Dots in the module name are converted to path separators (<code>require("lib.utils")</code> → <code>plugins/my-plugin/lib/utils.lua</code>). Path traversal attempts (<code>../</code>) raise a Lua error. Standard Lua packages (<code>string</code>, <code>table</code>, <code>math</code>, <code>os</code>) are always available.</p>

<h2>Multi-file plugin layout (recommended)</h2>
<pre><code>plugins/compile-action/
  plugin.toml
  main.lua              ← thin wiring: require sub-modules, register hooks/UI
  state.lua             ← shared mutable state (current repo, running job IDs)
  detect.lua            ← project type auto-detection (Maven/Gradle/npm/…)
  defaults.lua          ← default build configs per project type
  run_defaults.lua      ← default run configs per project type
  config/
    global.lua          ← global build settings CRUD + form
    project.lua         ← per-repo build settings CRUD + form
    run_global.lua      ← global run settings CRUD + form (+ auto_stop global default)
    run_project.lua     ← per-repo run settings CRUD + form (+ tomcat_home, auto_stop override)
    jdk.lua             ← JDK registry (shared by build + run)
  ui/
    combo.lua           ← build combo (Hammer icon)
    run_combo.lua       ← run combo (Play icon)</code></pre>
<pre class="language-lua">{@html highlight(`-- main.lua
local state     = require("state")
local combo     = require("ui.combo")
local run_combo = require("ui.run_combo")

arbor.events.on("on_plugin_load", function(ctx)
  combo.register()      -- 🔨 Build combo (right)
  run_combo.register()  -- ▶  Run combo (left)

  arbor.keybinding.register({ key = "F9", action = "compile:run", description = "Build selected" })
  arbor.keybinding.register({ key = "F5", action = "run:run",     description = "Run selected"   })
end)

arbor.events.on("on_repo_open", function(ctx)
  state.set_repo(ctx.path)
  combo.refresh()
  run_combo.refresh()
end)`, '.lua')}</pre>

<h2>Plugin dependencies</h2>
<p>
  A plugin can declare that it requires another plugin (for example, because it publishes an event on the bus that the other plugin reads). Add one <code>[[dependencies]]</code> entry per required plugin in your <code>plugin.toml</code>:
</p>
<pre class="language-toml">{@html highlight(`[[dependencies]]
name     = "compile-action"
version  = ">=1.0.0"   # semver requirement; empty = any version
optional = false        # when true, a mismatch is a warning, not an error

[[dependencies]]
name     = "auto-fetch"
version  = "^0.2.0"
optional = true`, 'toml')}</pre>
<p>Accepted semver operators: <code>=</code>, <code>&gt;</code>, <code>&gt;=</code>, <code>&lt;</code>, <code>&lt;=</code>, <code>~</code>, <code>^</code>, plus exact versions (<code>1.2.3</code>) and wildcards (<code>1.*</code>).</p>

<h3>Load ordering &amp; errors</h3>
<ul class="prop-list">
  <li><strong>Topo-sort</strong>At startup all manifests are topologically sorted so each plugin loads <em>after</em> its dependencies. Cycles are rejected with a descriptive error; involved plugins show greyed-out in the Plugin Manager.</li>
  <li><strong>Unmet dep</strong>Missing or version-mismatched dependency → plugin skipped, red banner on the card.</li>
  <li><strong>Optional</strong><code>optional = true</code> downgrades the error to a log warning. Your plugin still loads — guard calls that depend on the other plugin's presence.</li>
</ul>

<h3>Dependency graph &amp; cascade</h3>
<p>The Plugin Manager exposes a <strong>Network</strong> icon opening the <em>Plugin Dependency Graph</em> modal. Each plugin's expanded detail row also exposes:</p>
<div class="feature-grid two-col">
  <div class="feature-card">
    <div class="fc-title">Depends on</div>
    <div class="fc-desc">Plugins your plugin declares, with version requirements and optional/unmet tags.</div>
  </div>
  <div class="feature-card">
    <div class="fc-title">Required by</div>
    <div class="fc-desc">Plugins currently installed (loaded or dormant) that declare yours as a required dep.</div>
  </div>
</div>
<p>Enable / disable / uninstall actions cascade along the required-dependency edges. Optional deps don't participate.</p>
<ul class="prop-list">
  <li><strong>Disable</strong>Disabling a plugin first asks for confirmation, then disables every transitively-required dependent (leaves first). Re-enabling the target later does NOT auto-re-enable the dependents — flip them back on individually when you're ready.</li>
  <li><strong>Enable</strong>Enabling a plugin whose required deps are off asks for confirmation, then turns them on in topological order before the target. When a required dep is missing or unloadable, the action is refused and the modal lists the blockers.</li>
  <li><strong>Uninstall</strong>Uninstalling a plugin first disables every dependent (they stay installed) so they don't keep running against a vanished service / hook target.</li>
  <li><strong>Marketplace install</strong>Installing a plugin from the Marketplace pre-resolves its required deps against the catalog. The confirm modal shows "Will also install: …" so the cascade is downloaded in dep-first order; deps not present in the catalog block the install with a clear error.</li>
</ul>

<h2>Permissions reference</h2>
<p>Declared once in <code>[permissions]</code> of <code>plugin.toml</code>. Capability is enforced at Lua call-time — trying to use a disabled API raises a runtime error.</p>

<table>
  <thead><tr><th>Key</th><th>Value</th><th>Enables</th></tr></thead>
  <tbody>
    <tr><td><code>network</code></td><td>string[]</td><td>Allowed hostnames for <code>arbor.http.get</code>. Exact match or registrable suffix (<code>"maven.org"</code> permits <code>repo1.maven.org</code> and itself). Use <code>["*"]</code> for any host. Empty list = no network.</td></tr>
    <tr><td rowspan="3"><code>fs</code></td><td><code>"none"</code> <span class="badge badge-opt">default</span></td><td>No <code>arbor.fs.*</code> access</td></tr>
    <tr><td><code>"read"</code></td><td>Read-only filesystem ops (<code>read / list / glob / exists / is_file / is_dir</code>)</td></tr>
    <tr><td><code>"write"</code></td><td>Read + write (<code>write / append / touch / move / delete / copy / json_set / yaml_set / toml_set</code>)</td></tr>
    <tr><td rowspan="3"><code>fs_scope</code></td><td><code>[]</code> <span class="badge badge-opt">default</span></td><td>Sandboxed to the active repo's directory. <strong>Use <code>["*"]</code> instead when the plugin writes to user-picked paths via <code>arbor.ui.pick_file&#123; mode = "save" &#125;</code></strong> — the sandbox would otherwise reject anything outside the repo (e.g. <code>~/Downloads/foo.md</code>).</td></tr>
    <tr><td><code>["*"]</code></td><td>Unrestricted — any path on disk</td></tr>
    <tr><td><code>["/abs/path", …]</code></td><td>Allow these absolute paths in addition to the active repo</td></tr>
    <tr><td rowspan="4"><code>git</code></td><td><code>"none"</code> <span class="badge badge-opt">default</span></td><td>No <code>arbor.repo.*</code> / <code>arbor.notes.*</code> access</td></tr>
    <tr><td><code>"read"</code></td><td><code>arbor.repo.current / branch / is_dirty / remote / branches / tags</code> + <code>arbor.notes.list / get</code></td></tr>
    <tr><td><code>"write"</code></td><td>Read + non-destructive writes (<code>fetch_active_tab</code>, <code>clone</code>, <code>notes.set / delete</code>)</td></tr>
    <tr><td><code>"history_rewrite"</code></td><td>Write + destructive history ops (rebase, <code>reset --hard</code>, force-push, amend, filter-branch). Granted separately because these can permanently destroy work.</td></tr>
    <tr><td rowspan="3"><code>issues</code></td><td><code>"none"</code> <span class="badge badge-opt">default</span></td><td>No <code>arbor.issues.*</code> access</td></tr>
    <tr><td><code>"read"</code></td><td><code>arbor.issues.search()</code>, <code>arbor.issues.get()</code></td></tr>
    <tr><td><code>"write"</code></td><td>Read + <code>arbor.issues.transition()</code>, <code>arbor.issues.comment()</code></td></tr>
    <tr><td rowspan="3"><code>provider</code></td><td><code>"none"</code> <span class="badge badge-opt">default</span></td><td>No <code>arbor.mr.*</code> / <code>arbor.ci.*</code> access</td></tr>
    <tr><td><code>"read"</code></td><td><code>arbor.mr.list</code>, <code>arbor.mr.current_user</code>, <code>arbor.ci.runs</code> — credential-blind: tokens stay in the OS keyring</td></tr>
    <tr><td><code>"write"</code></td><td>Reserved for future MR/CI mutations (create / comment / retrigger)</td></tr>
    <tr><td rowspan="3"><code>toolchain</code></td><td><code>"none"</code> <span class="badge badge-opt">default</span></td><td>No <code>arbor.toolchain.*</code> access</td></tr>
    <tr><td><code>"read"</code></td><td><code>list</code>, <code>active</code>, <code>env</code>, <code>detect</code></td></tr>
    <tr><td><code>"write"</code></td><td>Read + <code>add</code>, <code>remove</code>, <code>set_active</code></td></tr>
    <tr><td rowspan="3"><code>terminal</code></td><td><code>"none"</code> <span class="badge badge-opt">default</span></td><td>No <code>arbor.terminal.exec()</code></td></tr>
    <tr><td><code>"any"</code></td><td>Any command allowed</td></tr>
    <tr><td><code>"commands"</code></td><td>Only basenames listed in <code>terminal_scope</code> allowed</td></tr>
    <tr><td rowspan="3"><code>env_read</code></td><td><code>true</code> <span class="badge badge-opt">default</span></td><td><code>os.getenv()</code> reads any environment variable</td></tr>
    <tr><td><code>false</code></td><td><code>os.getenv</code> is removed from the sandbox</td></tr>
    <tr><td><code>["PATH", "JAVA_HOME"]</code></td><td>Allowlist — only listed names return a value, others return <code>nil</code></td></tr>
    <tr><td><code>service_export</code></td><td>bool</td><td><code>arbor.service.export / unexport / list_own</code> — expose callable services</td></tr>
    <tr><td><code>service_call</code></td><td>bool</td><td><code>arbor.service.call / list</code> — invoke services from other plugins</td></tr>
    <tr><td><code>settings_read_others</code></td><td>bool</td><td><code>arbor.settings.read(plugin, key)</code> — read other plugins' globals (own settings always readable)</td></tr>
  </tbody>
</table>

<Callout variant="warning" title="Sandbox hardening">
  These Lua functions are removed from the sandbox: <code>os.execute</code>, <code>os.exit</code>, <code>os.remove</code>, <code>os.rename</code>, <code>io.*</code>, <code>load</code>, <code>loadfile</code>, <code>dofile</code>. The <code>terminal</code> permission is captured at plugin load time — it cannot be escalated by overwriting a Lua global.
</Callout>
