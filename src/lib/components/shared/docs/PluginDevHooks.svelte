<script lang="ts">
  import { highlight } from '$lib/utils/diff-formatter';
  import Callout from '$lib/components/shared/ui/Callout.svelte';
</script>

<h1>Plugin Development — Hooks &amp; Events</h1>
<p>
  Declare which hooks your plugin subscribes to via boolean flags in <code>[hooks]</code>.
  Register handlers in Lua with <code>arbor.events.on("hook_name", fn)</code>. The full
  hook catalog (with the ctx schema for each one) is also browseable at runtime via
  <code>arbor.hooks.list()</code> and <code>arbor.hooks.describe(name)</code>.
</p>

<h2>String enums used by the API</h2>
<pre class="language-lua">{@html highlight(`-- arbor.notify level
"info" | "success" | "warning" | "error"     -- default "info"

-- arbor.log.LEVELS — autocomplete-friendly aliases for the bare strings
arbor.log.LEVELS.DEBUG  -- "debug"
arbor.log.LEVELS.INFO   -- "info"
arbor.log.LEVELS.WARN   -- "warn"
arbor.log.LEVELS.ERROR  -- "error"

-- Manifest enum strings (used only inside plugin.toml — not at runtime)
-- terminal: "none" | "commands" | "any"
-- fs:       "none" | "read" | "write"
-- git:      "none" | "read" | "write" | "history_rewrite"
-- form variants: "default" | "primary" | "danger" | "ghost"`, '.lua')}</pre>

<h2>Hooks reference</h2>
<table class="shortcuts-table">
  <thead><tr><th>Hook (TOML key &amp; event name)</th><th>Context fields</th></tr></thead>
  <tbody>
    <tr><td colspan="2" style="color:var(--text-muted);font-size:0.78rem;padding-top:0.6rem">── Lifecycle ─────────────────────────────────────────────────────────────────</td></tr>
    <tr><td><code>on_plugin_load</code></td><td>plugin_name, dir, api_version</td></tr>
    <tr><td><code>on_repo_open</code></td><td>tab_id, path, name</td></tr>
    <tr><td><code>on_repo_close</code></td><td>tab_id, path, name</td></tr>
    <tr><td><code>on_repo_init</code></td><td>path, name, default_branch, provider, remote_url, has_readme, license, gitignore</td></tr>
    <tr><td><code>on_repo_deregistered</code></td><td>repo_id, path, name, reason</td></tr>
    <tr><td><code>on_project_missing</code></td><td>repo_id, path, name, reason ("missing" | "unreachable" | "not_a_repo")</td></tr>
    <tr><td><code>on_project_relocated</code></td><td>repo_id, old_path, new_path, name, remote_url</td></tr>
    <tr><td><code>on_tab_switch</code></td><td>tab_id</td></tr>
    <tr><td colspan="2" style="color:var(--text-muted);font-size:0.78rem;padding-top:0.6rem">── Git operations ────────────────────────────────────────────────────────────</td></tr>
    <tr><td><code>on_pre_commit</code></td><td>tab_id, message, amend — <strong>vetoable</strong> (return a string to block)</td></tr>
    <tr><td><code>on_commit</code></td><td>tab_id, oid, message, amend</td></tr>
    <tr><td><code>on_push</code></td><td>tab_id, remote, refspec, force</td></tr>
    <tr><td><code>on_pull</code></td><td>tab_id, remote</td></tr>
    <tr><td><code>on_fetch</code></td><td>tab_id, remote</td></tr>
    <tr><td><code>on_checkout</code></td><td>tab_id, branch <em>or</em> oid (detached)</td></tr>
    <tr><td colspan="2" style="color:var(--text-muted);font-size:0.78rem;padding-top:0.6rem">── Branch / tag ──────────────────────────────────────────────────────────────</td></tr>
    <tr><td><code>on_branch_create</code></td><td>tab_id, name, from_oid</td></tr>
    <tr><td><code>on_branch_delete</code></td><td>tab_id, name <em>or</em> names[] (bulk delete)</td></tr>
    <tr><td><code>on_branch_rename</code></td><td>tab_id, old_name, new_name</td></tr>
    <tr><td><code>on_tag_create</code></td><td>tab_id, name, oid, annotated</td></tr>
    <tr><td><code>on_tag_delete</code></td><td>tab_id, name</td></tr>
    <tr><td colspan="2" style="color:var(--text-muted);font-size:0.78rem;padding-top:0.6rem">── Stash ─────────────────────────────────────────────────────────────────────</td></tr>
    <tr><td><code>on_stash_push</code></td><td>tab_id, index, message, include_untracked</td></tr>
    <tr><td><code>on_stash_pop</code></td><td>tab_id, index, drop (true=pop, false=apply)</td></tr>
    <tr><td colspan="2" style="color:var(--text-muted);font-size:0.78rem;padding-top:0.6rem">── Rebase ────────────────────────────────────────────────────────────────────</td></tr>
    <tr><td><code>on_rebase_start</code></td><td>tab_id, base, action_count</td></tr>
    <tr><td><code>on_rebase_abort</code></td><td>tab_id</td></tr>
    <tr><td colspan="2" style="color:var(--text-muted);font-size:0.78rem;padding-top:0.6rem">── Git Flow ──────────────────────────────────────────────────────────────────</td></tr>
    <tr><td><code>on_flow_init</code></td><td>tab_id</td></tr>
    <tr><td><code>on_flow_feature_start</code></td><td>tab_id, name</td></tr>
    <tr><td><code>on_flow_feature_finish</code></td><td>tab_id, name</td></tr>
    <tr><td><code>on_flow_release_start</code></td><td>tab_id, version</td></tr>
    <tr><td><code>on_flow_release_finish</code></td><td>tab_id, version</td></tr>
    <tr><td><code>on_flow_hotfix_start</code></td><td>tab_id, name</td></tr>
    <tr><td><code>on_flow_hotfix_finish</code></td><td>tab_id, name</td></tr>
    <tr><td colspan="2" style="color:var(--text-muted);font-size:0.78rem;padding-top:0.6rem">── Pipelines ─────────────────────────────────────────────────────────────────</td></tr>
    <tr><td><code>on_pipeline_run_request</code></td><td>pipeline_id, tab_id? — fired only when the user presses Play on a <em>stub</em> def (empty <code>stages</code>); defs with non-empty stages are replayed directly. Handler must compile stages and call <code>arbor.pipeline.run</code></td></tr>
    <tr><td><code>on_pipeline_started</code></td><td>run_id, pipeline_id, plugin</td></tr>
    <tr><td><code>on_pipeline_step_done</code></td><td>run_id, stage, step, exit_code</td></tr>
    <tr><td><code>on_pipeline_done</code></td><td>run_id, plugin, status</td></tr>
    <tr><td colspan="2" style="color:var(--text-muted);font-size:0.78rem;padding-top:0.6rem">── Merge Requests / Pull Requests ────────────────────────────────────────────</td></tr>
    <tr><td><code>on_mr_opened</code></td><td>number, title, source_branch, target_branch, provider</td></tr>
    <tr><td><code>on_mr_merged</code></td><td>number, provider</td></tr>
    <tr><td><code>on_mr_updated</code></td><td>number, provider</td></tr>
    <tr><td colspan="2" style="color:var(--text-muted);font-size:0.78rem;padding-top:0.6rem">── Issues ────────────────────────────────────────────────────────────────────</td></tr>
    <tr><td><code>on_issue_linked</code></td><td>issue_id, identifier, sha, branch</td></tr>
    <tr><td><code>on_issue_transitioned</code></td><td>issue_id, identifier, from_status, to_status</td></tr>
    <tr><td colspan="2" style="color:var(--text-muted);font-size:0.78rem;padding-top:0.6rem">── Git notes ─────────────────────────────────────────────────────────────────</td></tr>
    <tr><td><code>on_note_saved</code></td><td>tab_id, commit_oid, namespace, plugin? (set when fired from Lua)</td></tr>
    <tr><td><code>on_note_deleted</code></td><td>tab_id, commit_oid, namespace, plugin? (set when fired from Lua)</td></tr>
    <tr><td colspan="2" style="color:var(--text-muted);font-size:0.78rem;padding-top:0.6rem">── Workspaces ────────────────────────────────────────────────────────────────</td></tr>
    <tr><td><code>on_workspace_created</code></td><td>id, name, color_idx, group_id, repo_ids, repo_count</td></tr>
    <tr><td><code>on_workspace_updated</code></td><td>id, name, color_idx, group_id, repo_ids, repo_count</td></tr>
    <tr><td><code>on_workspace_deleted</code></td><td>id, name, color_idx, group_id, repo_ids, repo_count</td></tr>
    <tr><td><code>on_workspace_switched</code></td><td>id, name, color_idx, repo_ids, from_id? (previous workspace)</td></tr>
    <tr><td><code>on_workspace_repo_added</code></td><td>workspace_id, repo_id</td></tr>
    <tr><td><code>on_workspace_repo_removed</code></td><td>workspace_id, repo_id</td></tr>
    <tr><td colspan="2" style="color:var(--text-muted);font-size:0.78rem;padding-top:0.6rem">── Security ──────────────────────────────────────────────────────────────────</td></tr>
    <tr><td><code>on_security_summary_loaded</code></td><td>tab_id, provider, counts, total, risk_label?, web_url? (counts are active-only)</td></tr>
    <tr><td><code>on_security_finding_state_changed</code></td><td>tab_id, finding_id, severity, from_state?, to_state, title?, web_url? (plugin-cooperation channel)</td></tr>
    <tr><td colspan="2" style="color:var(--text-muted);font-size:0.78rem;padding-top:0.6rem">── Theme / branding ──────────────────────────────────────────────────────────</td></tr>
    <tr><td><code>on_theme_changed</code></td><td>theme_id, theme_name, vars (merged effective stylesheet), source ("user"|"plugin"|"init")</td></tr>
    <tr><td colspan="2" style="color:var(--text-muted);font-size:0.78rem;padding-top:0.6rem">── Schedulers ────────────────────────────────────────────────────────────────</td></tr>
    <tr><td><code>arbor.scheduler.register</code> (action name)</td><td>Spring-style triggers: <code>fixed_rate</code> / <code>fixed_delay</code> / <code>cron</code>. Manifest opt-in: <code>[scheduler] enabled = true</code></td></tr>
  </tbody>
</table>

<h2>Vetoable hooks — <code>on_pre_commit</code></h2>
<p>
  A small set of hooks runs <em>before</em> the host operation and lets
  any handler abort it. Today only <code>on_pre_commit</code> uses this
  pattern; future additions (e.g. <code>on_pre_push</code>) will follow
  the same convention.
</p>
<ul>
  <li>Returning a non-empty <strong>string</strong> from the handler
      blocks the operation. The string is used as the abort reason
      and shown to the user.</li>
  <li>Returning <code>false</code> blocks without a stated reason.</li>
  <li>Returning <code>nil</code> (or no value) lets the operation
      proceed.</li>
  <li>Multiple plugins each see the same payload; <strong>every</strong>
      veto is concatenated into the final error message.</li>
</ul>
<pre class="language-lua">{@html highlight(`arbor.events.on("on_pre_commit", function(ctx)
  -- ctx = { tab_id, message, amend }
  if #ctx.message > 200 then
    return "Subject too long: " .. #ctx.message .. " chars (max 200)."
  end
  -- nothing returned → commit proceeds
end)`, '.lua')}</pre>

<h2>arbor.events — subscribe and emit</h2>
<p>
  One namespace for both built-in lifecycle hooks (<code>on_repo_open</code>, <code>on_commit</code>, …) and plugin-defined events. Subscribers don't have to distinguish the two: every event flows through the same <code>arbor.events.on(name, fn)</code>.
</p>
<p>
  <strong>Naming rule for plugin events:</strong> events are always published under the <em>publisher's</em> plugin name. If you call <code>arbor.events.emit("build-done", ...)</code> from the plugin <code>compile-action</code>, Arbor dispatches <code>compile-action:build-done</code> to every subscriber. If you include a colon yourself, the prefix must match your own plugin name — otherwise a runtime error is raised (this prevents one plugin from spoofing another's events).
</p>
<pre class="language-lua">{@html highlight(`-- ── Publisher: plugins/compile-action/main.lua ─────────────────────────────────
arbor.events.on("compile:run", function(_)
  local job, err = arbor.job.spawn({
    name    = "Build",
    command = "make",
    cwd     = arbor.repo.current(),
  })
  if not job then arbor.log.warn("spawn failed: " .. err); return end
  job:ok(function(r)  arbor.events.emit("build-done", { success = true,  exit_code = r.exit_code, repo = arbor.repo.current() }) end)
     :err(function(r) arbor.events.emit("build-done", { success = false, exit_code = (r and r.exit_code) or -1, repo = arbor.repo.current() }) end)
end)

-- ── Subscriber: plugins/auto-notify/main.lua ──────────────────────────────────
arbor.events.on("compile-action:build-done", function(ctx)
  if ctx.success then
    arbor.notify{ title = "Build OK", message = "Finished cleanly", level = "success" }
  else
    arbor.notify{ title = "Build failed", message = "Exit " .. ctx.exit_code, level = "error" }
  end
end)`, '.lua')}</pre>
<p>
  Payloads are serialised to JSON once on the emitting side and delivered as native Lua tables to every subscriber.
</p>
<p>
  <strong>Delivery is asynchronous.</strong> <code>emit</code> dispatches on a background thread so it can safely be called from inside a hook handler (where the plugin host mutex is already held). Don't assume subscribers have run by the time <code>emit</code> returns — if you need to react to completion, have the subscriber emit its own follow-up event.
</p>

<h3>arbor.service — cross-plugin RPC</h3>
<p>
  Where <code>arbor.events.emit</code> is fire-and-forget, <code>arbor.service</code> is
  request / response. A plugin exports named functions; other plugins call them
  with arguments and get the return value as a Promise. Calls always run on a
  background thread and never block the caller, so they're safe to invoke from
  inside any hook handler.
</p>
<pre class="language-lua">{@html highlight(`-- Provider: plugins/greeter/main.lua ------------------------------------------
-- manifest.toml → [permissions] service_export = true
arbor.service.export("greet", function(args)
  return "hello " .. (args.name or "world")
end)

-- Consumer: plugins/caller/main.lua --------------------------------------------
-- manifest.toml → [permissions] service_call = true
arbor.service.call("greeter.greet", { name = "Arbor" })
  :ok(function(r) arbor.log.info(r) end)                  -- "hello Arbor"
  :err(function(e) arbor.log.warn(e.kind .. ": " .. e.message) end)

-- Inside an async.run coroutine you can await sequentially:
arbor.async.run(function()
  local r, err = arbor.async.await(arbor.service.call("greeter.greet", { name = "Arbor" }))
  if err then arbor.log.warn(err.message); return end
  arbor.log.info(r)
end)`, '.lua')}</pre>
<h4>Typed error kinds</h4>
<p>The promise rejects with a table <code>&#123; kind, message &#125;</code>; <code>kind</code> is one of:</p>
<ul>
  <li><code>not_found</code> — the target plugin isn't loaded, or the requested method isn't registered</li>
  <li><code>plugin_disabled</code> — the target plugin is installed but disabled in the Plugin Manager</li>
  <li><code>handler_error</code> — the provider's handler raised while executing (message carries the Lua error)</li>
</ul>
<p>
  An optional third <code>cb</code> argument still works as zucchero: it fires alongside
  the promise with <code>(ok, value_or_err)</code>. Omit it (and the promise) entirely for
  "fire and forget" calls whose outcome you don't care about.
</p>
<h4>Debug helpers</h4>
<pre class="language-lua">{@html highlight(`arbor.service.list()        -- every "<plugin>.<method>" exported by any enabled plugin
arbor.service.list_own()    -- only the services this plugin has exported`, '.lua')}</pre>
<Callout variant="info" title="Delivery semantics">
  Each call spawns a short-lived worker thread that acquires the plugin host mutex, runs the target handler, then invokes the caller's callback — in that order, under the same lock. The callback executes on the worker thread, so don't assume Svelte-side state is in any particular state; prefer to <code>arbor.events.emit</code> a follow-up event for UI reactions.
</Callout>

<h3>Wildcard subscriptions</h3>
<p>
  The event name passed to <code>arbor.events.on</code> may contain one or more <code>*</code> characters. Each <code>*</code> matches any sequence of characters — including empty strings and colon / dot separators — with no segment boundaries. Literal strings without <code>*</code> still require an exact match.
</p>
<pre class="language-lua">{@html highlight(`-- Debug: log every event fired anywhere
arbor.events.on("*", function(ctx)
  arbor.log.debug("bus event received: " .. arbor.json.encode(ctx))
end)

-- Listen to all events from one plugin
arbor.events.on("compile-action:*", function(ctx)
  -- matches "compile-action:build-done", "compile-action:started", …
end)

-- Match a suffix
arbor.events.on("*:build-done", function(ctx) ... end)`, '.lua')}</pre>
<Callout variant="info" title="Note">
  A plugin with at least one wildcard subscription bypasses the manifest hook filter — it will receive all built-in lifecycle hooks too (<code>on_commit</code>, <code>on_repo_open</code>, …) even if they aren't declared under <code>[hooks]</code>. Handlers must tolerate varied payload shapes.
</Callout>

<h3>Discovering hooks at runtime — <code>arbor.hooks</code></h3>
<p>
  Every built-in hook ships with a machine-readable schema describing the
  <code>ctx</code> table its handlers receive. Use it to generate docs, build
  validators, or pick the right hook to subscribe to without leaving your editor.
</p>
<pre class="language-lua">{@html highlight(`-- List every built-in hook
for _, def in ipairs(arbor.hooks.list()) do
  arbor.log.info(def.category .. " :: " .. def.name)
end

-- Inspect one hook
local d = arbor.hooks.describe("on_repo_open")
-- d = {
--   name        = "on_repo_open",
--   category    = "repo",
--   description = "Fired when the user opens a repo …",
--   ctx = {
--     { name="tab_id", type="string", required=true, description="…" },
--     { name="path",   type="string", required=true, description="…" },
--     { name="name",   type="string", required=true, description="…" },
--   },
-- }`, '.lua')}</pre>
<p>
  Action hooks fired via <code>arbor.events.emit</code>, <code>arbor.command.register</code>,
  or <code>arbor.job.spawn&lbrace;on_done=…&rbrace;</code> are <em>not</em> in the catalog — they're plugin-defined.
  <code>describe()</code> returns <code>nil</code> for those.
</p>
