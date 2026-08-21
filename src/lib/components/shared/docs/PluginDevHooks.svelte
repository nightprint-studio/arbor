<script lang="ts">
  import { highlight } from '$lib/utils/diff-formatter';
  import Callout from '$lib/components/shared/ui/Callout.svelte';
</script>

<h1>Plugin Development — Hooks &amp; Events</h1>
<p>
  Declare which hooks your plugin subscribes to via boolean flags in <code>[hooks]</code>.
  Register handlers in Lua with <code>arbor.events.on(name, fn)</code> — that is the only
  subscribe entry point; there is no <code>arbor.on</code>. The full hook catalog (with the
  ctx schema for each one) is also browseable at runtime via
  <code>arbor.hooks.list()</code> and <code>arbor.hooks.describe(name)</code>.
</p>

<h2>Hook names — <code>&lt;product&gt;:&lt;event&gt;</code></h2>
<p>
  Every hook is named <code>&lt;product&gt;:&lt;event&gt;</code>: <code>corvus:commit</code>,
  <code>garrulus:note_saved</code>, <code>arbor:plugin_load</code>. The prefix is the id of the
  product that owns the concept — <code>corvus</code> for git, <code>garrulus</code> for note
  vaults, <code>arbor</code> for the host itself, plus <code>pipeline</code> for the pipeline
  engine. It is the same <code>&lt;namespace&gt;:&lt;event&gt;</code> shape that
  <code>arbor.events.emit</code> already uses for plugin-defined events, so subscribers never
  have to tell the two apart.
</p>
<p>
  The event half never repeats the namespace: it is <code>garrulus:note_saved</code>, not
  <code>garrulus:vault_note_saved</code> — the prefix is already the disambiguator.
</p>
<p>
  <strong>The prefix is optional when you subscribe.</strong> An unqualified name is resolved
  against the product hosting your plugin — and, when that product has no such hook but the
  host namespace does, against <code>arbor:</code>. A name that already carries a
  <code>:</code> is never rewritten:
</p>
<pre class="language-lua">{@html highlight(`-- Inside a Garrulus plugin
arbor.events.on("note_saved", fn)     -- "garrulus:note_saved"  (host product)
arbor.events.on("plugin_load", fn)    -- "arbor:plugin_load"    (host namespace)
arbor.events.on("corvus:commit", fn)  -- "corvus:commit"        (left as written)`, '.lua')}</pre>
<p>
  The <code>arbor:</code> fallback is what makes host hooks portable. Plugin lifecycle, views,
  the theme and which project is open belong to no product, so without it the same
  <code>main.lua</code> line would mean <code>corvus:plugin_load</code> under one host and
  <code>garrulus:plugin_load</code> under another — and neither of those exists.
</p>
<p>
  Write the short form for your own product's hooks and for the host lifecycle ones; write the
  qualified form when the name benefits from the context, and <em>always</em> when listening to
  another product — a qualified name is never rewritten, so subscribing across products is
  simply a matter of spelling the namespace out. The prefix is what keeps
  <code>corvus:note_saved</code> (a git note was written) and <code>garrulus:note_saved</code>
  (a vault note was written) apart: two unrelated events that would otherwise share one name.
</p>
<p>
  The name you subscribe to is checked against the hook catalog once it is resolved. A name
  that matches nothing — a typo, or a product prefix that is not loaded — is reported in the
  plugin log instead of silently never firing. Names containing <code>*</code> are wildcards
  and are matched, not validated — and never prefixed, so a pattern means exactly what it
  says: <code>arbor.events.on("corvus:*", fn)</code> receives every Corvus hook and
  <code>arbor.events.on("garrulus:*", fn)</code> every Garrulus one, both of them still
  working when new hooks are added to those products.
</p>
<p>
  Manifest keys under <code>[hooks]</code> are the same names. Quote them, because a colon is
  not legal in a bare TOML key:
</p>
<pre class="language-toml">{@html highlight(`[hooks]
"arbor:plugin_load"  = true
"corvus:commit"      = true
"garrulus:sync_done" = true
"pipeline:done"      = true`, 'toml')}</pre>

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
    <tr><td colspan="2" style="color:var(--text-muted);font-size:0.78rem;padding-top:0.6rem">── Host lifecycle (<code>arbor:</code>) — fired under every product ───────────</td></tr>
    <tr><td><code>arbor:plugin_load</code></td><td>plugin_name, dir, api_version</td></tr>
    <tr><td><code>arbor:plugin_unload</code></td><td>plugin_name — fired on shutdown, disable and reload</td></tr>
    <tr><td><code>arbor:view_open</code></td><td>view_id, label? — fired on the owning plugin when one of its <code>add_view</code> views opens; respond with <code>set_panel_content</code></td></tr>
    <tr><td><code>arbor:view_close</code></td><td>view_id, label? — fired when the view is closed (toggled off, replaced, or plugin reloaded)</td></tr>
    <tr><td><code>arbor:theme_changed</code></td><td>theme_id, theme_name, vars (merged effective stylesheet), source ("user"|"plugin"|"init")</td></tr>
    <tr><td colspan="2" style="color:var(--text-muted);font-size:0.78rem;padding-top:0.6rem">── Bennu: the editor (<code>bennu:</code>) ───────────</td></tr>
    <tr><td><code>bennu:file_opened</code></td><td>path, name, ext? — the editor's active file changed (tab switched, file opened, reopened from history). What a panel about the file being edited follows.</td></tr>
    <tr><td><code>bennu:file_closed</code></td><td>— fired when the last editor tab closes and nothing is being edited</td></tr>
    <tr><td colspan="2" style="color:var(--text-muted);font-size:0.78rem;padding-top:0.6rem">── Host: which project is open (<code>arbor:</code>) — every product, not just git ──</td></tr>
    <tr><td><code>arbor:repo_open</code></td><td>tab_id, path, name — fired on open and again after a plugin reload</td></tr>
    <tr><td><code>arbor:repo_close</code></td><td>tab_id, path, name</td></tr>
    <tr><td><code>arbor:tab_switch</code></td><td>tab_id, path, name</td></tr>
    <tr><td colspan="2" style="color:var(--text-muted);font-size:0.78rem;padding-top:0.6rem">── Corvus: repo registry ─────────────────────────────────────────────────────</td></tr>
    <tr><td><code>corvus:repo_init</code></td><td>path, name, default_branch, provider, remote_url, pushed, has_readme, license, gitignore</td></tr>
    <tr><td><code>corvus:repo_deregistered</code></td><td>repo_id, path, name, reason</td></tr>
    <tr><td><code>corvus:project_missing</code></td><td>repo_id, path, name, reason ("missing" | "unreachable" | "not_a_repo")</td></tr>
    <tr><td><code>corvus:project_relocated</code></td><td>repo_id, old_path, new_path, name, remote_url</td></tr>
    <tr><td colspan="2" style="color:var(--text-muted);font-size:0.78rem;padding-top:0.6rem">── Corvus: git operations ────────────────────────────────────────────────────</td></tr>
    <tr><td><code>corvus:pre_commit</code></td><td>tab_id, message, amend — <strong>vetoable</strong> (return a string to block)</td></tr>
    <tr><td><code>corvus:commit</code></td><td>tab_id, oid, message, amend</td></tr>
    <tr><td><code>corvus:push</code></td><td>tab_id, remote, refspec, force</td></tr>
    <tr><td><code>corvus:pull</code></td><td>tab_id, remote</td></tr>
    <tr><td><code>corvus:fetch</code></td><td>tab_id, remote</td></tr>
    <tr><td><code>corvus:checkout</code></td><td>tab_id, branch <em>or</em> oid (detached)</td></tr>
    <tr><td colspan="2" style="color:var(--text-muted);font-size:0.78rem;padding-top:0.6rem">── Corvus: branch / tag ──────────────────────────────────────────────────────</td></tr>
    <tr><td><code>corvus:branch_create</code></td><td>tab_id, name, from_oid</td></tr>
    <tr><td><code>corvus:branch_delete</code></td><td>tab_id, name <em>or</em> names[] (bulk delete)</td></tr>
    <tr><td><code>corvus:branch_rename</code></td><td>tab_id, old_name, new_name</td></tr>
    <tr><td><code>corvus:tag_create</code></td><td>tab_id, name, oid, annotated</td></tr>
    <tr><td><code>corvus:tag_delete</code></td><td>tab_id, name</td></tr>
    <tr><td colspan="2" style="color:var(--text-muted);font-size:0.78rem;padding-top:0.6rem">── Corvus: stash ─────────────────────────────────────────────────────────────</td></tr>
    <tr><td><code>corvus:stash_push</code></td><td>tab_id, index, message, include_untracked</td></tr>
    <tr><td><code>corvus:stash_pop</code></td><td>tab_id, index, drop (true=pop, false=apply)</td></tr>
    <tr><td colspan="2" style="color:var(--text-muted);font-size:0.78rem;padding-top:0.6rem">── Corvus: rebase ────────────────────────────────────────────────────────────</td></tr>
    <tr><td><code>corvus:rebase_start</code></td><td>tab_id, base, action_count</td></tr>
    <tr><td><code>corvus:rebase_abort</code></td><td>tab_id</td></tr>
    <tr><td colspan="2" style="color:var(--text-muted);font-size:0.78rem;padding-top:0.6rem">── Corvus: Git Flow ──────────────────────────────────────────────────────────</td></tr>
    <tr><td><code>corvus:flow_init</code></td><td>tab_id</td></tr>
    <tr><td><code>corvus:flow_feature_start</code></td><td>tab_id, name</td></tr>
    <tr><td><code>corvus:flow_feature_finish</code></td><td>tab_id, name</td></tr>
    <tr><td><code>corvus:flow_release_start</code></td><td>tab_id, version</td></tr>
    <tr><td><code>corvus:flow_release_finish</code></td><td>tab_id, version</td></tr>
    <tr><td><code>corvus:flow_hotfix_start</code></td><td>tab_id, name</td></tr>
    <tr><td><code>corvus:flow_hotfix_finish</code></td><td>tab_id, name</td></tr>
    <tr><td colspan="2" style="color:var(--text-muted);font-size:0.78rem;padding-top:0.6rem">── Pipeline engine (<code>pipeline:</code>) — its own namespace, not Corvus's ──</td></tr>
    <tr><td><code>pipeline:run_request</code></td><td>pipeline_id, tab_id — <strong>targeted</strong> at the plugin that owns the pipeline, never broadcast. Fired only when the user presses Play on a <em>stub</em> def (empty <code>stages</code>); defs with non-empty stages are replayed directly. Handler must compile stages and call <code>arbor.pipeline.run</code>; a plugin that declares a stub and does not subscribe gets a launch error</td></tr>
    <tr><td><code>pipeline:started</code></td><td>run_id, pipeline_id, plugin — also fired when a run resumes</td></tr>
    <tr><td><code>pipeline:step_done</code></td><td>run_id, plugin, stage_id, step_id, step_name, status ("success"|"failure"|"skipped"|"cancelled"), exit_code?</td></tr>
    <tr><td><code>pipeline:done</code></td><td>run_id, pipeline_id, plugin, status ("success"|"failure"|"cancelled")</td></tr>
    <tr><td colspan="2" style="color:var(--text-muted);font-size:0.78rem;padding-top:0.6rem">── Corvus: Merge Requests / Pull Requests ────────────────────────────────────</td></tr>
    <tr><td><code>corvus:mr_opened</code></td><td>number, title, source_branch, target_branch, provider, author, web_url</td></tr>
    <tr><td><code>corvus:mr_merged</code></td><td>number, provider</td></tr>
    <tr><td><code>corvus:mr_updated</code></td><td>number, provider</td></tr>
    <tr><td colspan="2" style="color:var(--text-muted);font-size:0.78rem;padding-top:0.6rem">── Corvus: issues ────────────────────────────────────────────────────────────</td></tr>
    <tr><td><code>corvus:issue_linked</code></td><td>provider ("linear"|"jira"), issue_id — <em>reserved</em>: in the catalog and subscribable, but no host code emits it</td></tr>
    <tr><td><code>corvus:issue_transitioned</code></td><td>provider, issue_id, from_state?, to_state — <em>reserved</em>: no host code emits it</td></tr>
    <tr><td colspan="2" style="color:var(--text-muted);font-size:0.78rem;padding-top:0.6rem">── Corvus: git notes ─────────────────────────────────────────────────────────</td></tr>
    <tr><td><code>corvus:note_saved</code></td><td>tab_id, commit_oid, namespace, plugin? (set when fired from Lua) — a <em>git</em> note; a vault note is <code>garrulus:note_saved</code></td></tr>
    <tr><td><code>corvus:note_deleted</code></td><td>tab_id, commit_oid, namespace, plugin? (set when fired from Lua) — a <em>git</em> note; a vault note is <code>garrulus:note_deleted</code></td></tr>
    <tr><td colspan="2" style="color:var(--text-muted);font-size:0.78rem;padding-top:0.6rem">── Corvus: workspaces ────────────────────────────────────────────────────────</td></tr>
    <tr><td><code>corvus:workspace_created</code></td><td>id, name, color_idx, group_id, repo_ids, repo_count</td></tr>
    <tr><td><code>corvus:workspace_updated</code></td><td>id, name, color_idx, group_id, repo_ids, repo_count</td></tr>
    <tr><td><code>corvus:workspace_deleted</code></td><td>id, name</td></tr>
    <tr><td><code>corvus:workspace_switched</code></td><td>id, name, color_idx, group_id, repo_ids, repo_count</td></tr>
    <tr><td><code>corvus:workspace_repo_added</code></td><td>workspace_id, repo_id</td></tr>
    <tr><td><code>corvus:workspace_repo_removed</code></td><td>workspace_id, repo_id</td></tr>
    <tr><td colspan="2" style="color:var(--text-muted);font-size:0.78rem;padding-top:0.6rem">── Corvus: linked worktrees ──────────────────────────────────────────────────</td></tr>
    <tr><td><code>corvus:worktree_link_sync_started</code></td><td>link_id, link_name, initiator_repo_id, target_branch</td></tr>
    <tr><td><code>corvus:worktree_link_sync_done</code></td><td>link_id, link_name, initiator_repo_id, target_branch, results</td></tr>
    <tr><td><code>corvus:worktree_link_member_added</code></td><td>link_id, repo_id</td></tr>
    <tr><td><code>corvus:worktree_link_member_removed</code></td><td>link_id, repo_id</td></tr>
    <tr><td colspan="2" style="color:var(--text-muted);font-size:0.78rem;padding-top:0.6rem">── Corvus: security ──────────────────────────────────────────────────────────</td></tr>
    <tr><td><code>corvus:security_summary_loaded</code></td><td>tab_id, provider, counts, total, risk_label?, web_url? (counts are active-only)</td></tr>
    <tr><td><code>corvus:security_finding_state_changed</code></td><td>tab_id, finding_id, severity, from_state?, to_state, title?, web_url? (plugin-cooperation channel)</td></tr>
    <tr><td colspan="2" style="color:var(--text-muted);font-size:0.78rem;padding-top:0.6rem">── Garrulus: vault ───────────────────────────────────────────────────────────</td></tr>
    <tr><td><code>garrulus:vault_opened</code></td><td>vault_id, path (absolute vault root), name, note_count</td></tr>
    <tr><td><code>garrulus:vault_closed</code></td><td>path (absolute vault root) — not fired when no vault was open</td></tr>
    <tr><td><code>garrulus:type_applied</code></td><td>path, type (note type id)</td></tr>
    <tr><td colspan="2" style="color:var(--text-muted);font-size:0.78rem;padding-top:0.6rem">── Garrulus: notes ───────────────────────────────────────────────────────────</td></tr>
    <tr><td><code>garrulus:note_created</code></td><td>path, source? ("trash" when restored from the vault trash)</td></tr>
    <tr><td><code>garrulus:note_saved</code></td><td>path, bytes? (ordinary save), source? ("conflict" when the remote side was adopted)</td></tr>
    <tr><td><code>garrulus:note_renamed</code></td><td>old_path, new_path — wikilinks are rewritten as ordinary saves, not by this hook</td></tr>
    <tr><td><code>garrulus:note_deleted</code></td><td>path, trash_id (the note is in the vault trash, not gone)</td></tr>
    <tr><td colspan="2" style="color:var(--text-muted);font-size:0.78rem;padding-top:0.6rem">── Garrulus: sync ────────────────────────────────────────────────────────────</td></tr>
    <tr><td><code>garrulus:sync_started</code></td><td>op ("pull"|"push"|"sync"), notes? (push batch size; 0 = everything changed)</td></tr>
    <tr><td><code>garrulus:sync_done</code></td><td>op, applied? , conflicts? (pull and sync only) — fired only on success</td></tr>
    <tr><td><code>garrulus:sync_conflict</code></td><td>count — fired before the matching <code>garrulus:sync_done</code></td></tr>
    <tr><td colspan="2" style="color:var(--text-muted);font-size:0.78rem;padding-top:0.6rem">── Schedulers ────────────────────────────────────────────────────────────────</td></tr>
    <tr><td><code>arbor.scheduler.register</code> (action name)</td><td>Spring-style triggers: <code>fixed_rate</code> / <code>fixed_delay</code> / <code>cron</code>. Manifest opt-in: <code>[scheduler] enabled = true</code></td></tr>
  </tbody>
</table>

<h2>Vetoable hooks — <code>corvus:pre_commit</code></h2>
<p>
  A vetoable hook runs <em>before</em> the host operation and lets any handler abort it.
  <code>corvus:pre_commit</code> is the one hook that works this way; the
  <code>pre_</code> prefix marks the convention.
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
<pre class="language-lua">{@html highlight(`arbor.events.on("corvus:pre_commit", function(ctx)
  -- ctx = { tab_id, message, amend }
  if #ctx.message > 200 then
    return "Subject too long: " .. #ctx.message .. " chars (max 200)."
  end
  -- nothing returned → commit proceeds
end)`, '.lua')}</pre>

<h2>arbor.events — subscribe and emit</h2>
<p>
  One namespace for both built-in hooks (<code>arbor:repo_open</code>, <code>corvus:commit</code>, …) and plugin-defined events. Both are <code>&lt;namespace&gt;:&lt;event&gt;</code>, so subscribers don't have to distinguish the two: everything flows through the same <code>arbor.events.on(name, fn)</code>.
</p>
<p>
  <strong>Naming rule for plugin events:</strong> events are always published under the <em>publisher's</em> plugin name. If you call <code>arbor.events.emit("build-done", ...)</code> from the plugin <code>compile-action</code>, Arbor dispatches <code>compile-action:build-done</code> to every subscriber. If you include a colon yourself, the prefix must match your own plugin name — otherwise a runtime error is raised (this prevents one plugin from spoofing another's events). Built-in hooks follow the same shape with the <em>product</em> id as the prefix, which is why a plugin can never publish an event that collides with one.
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
<p>
  <code>list()</code> is sorted by plugin, then method, and omits the exports of a
  disabled plugin — <code>call</code> would refuse them anyway.
</p>
<Callout variant="info" title="Delivery semantics">
  Each call spawns a short-lived worker thread that acquires the plugin host mutex, runs the target handler, then invokes the caller's callback — in that order, under the same lock. The callback executes on the worker thread, so don't assume Svelte-side state is in any particular state; prefer to <code>arbor.events.emit</code> a follow-up event for UI reactions.
</Callout>

<h3>Wildcard subscriptions</h3>
<p>
  The event name passed to <code>arbor.events.on</code> may contain one or more <code>*</code> characters. Each <code>*</code> matches any sequence of characters — including empty strings and colon / dot separators — with no segment boundaries. Literal strings without <code>*</code> still require an exact match. A pattern is matched as written: no product prefix is inserted into it, so <code>"*"</code> means every event on the bus and <code>"note_saved"</code> (no star) is still resolved against the host product.
</p>
<pre class="language-lua">{@html highlight(`-- Debug: log every event fired anywhere
arbor.events.on("*", function(ctx)
  arbor.log.debug("bus event received: " .. arbor.json.encode(ctx))
end)

-- Every hook of one product — keeps working when new ones are added
arbor.events.on("garrulus:*", function(ctx) ... end)

-- Listen to all events from one plugin
arbor.events.on("compile-action:*", function(ctx)
  -- matches "compile-action:build-done", "compile-action:started", …
end)

-- Match a suffix across products: git notes and vault notes alike
arbor.events.on("*:note_saved", function(ctx) ... end)`, '.lua')}</pre>
<Callout variant="info" title="Note">
  A plugin with at least one wildcard subscription bypasses the manifest hook filter — it will receive all built-in hooks too (<code>corvus:commit</code>, <code>arbor:repo_open</code>, …) even if they aren't declared under <code>[hooks]</code>. Handlers must tolerate varied payload shapes.
</Callout>

<h3>Discovering hooks at runtime — <code>arbor.hooks</code></h3>
<p>
  Every built-in hook ships with a machine-readable schema describing the
  <code>ctx</code> table its handlers receive. Use it to generate docs, build
  validators, or pick the right hook to subscribe to without leaving your editor.
</p>
<p>
  <code>list()</code> returns the whole catalog — every product's hooks, not just the host
  your plugin runs under. Filter on the namespace half of <code>def.name</code> when you only
  want the ones the current host can actually fire.
</p>
<pre class="language-lua">{@html highlight(`-- List every built-in hook
for _, def in ipairs(arbor.hooks.list()) do
  arbor.log.info(def.category .. " :: " .. def.name)
end

-- Inspect one hook — the prefix is optional here too
local d = arbor.hooks.describe("arbor:repo_open")
-- d = {
--   name        = "arbor:repo_open",
--   category    = "repo",
--   description = "Fired when the user opens a project …",
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
