<script lang="ts">
  import Callout from '$lib/components/shared/ui/Callout.svelte';
</script>

<h1>Missing &amp; Relocated Projects</h1>

<p class="doc-lead">
  When a registered project's folder is no longer available on disk — deleted, moved, on a drive
  that's offline — Arbor keeps the tab visible in tombstone state instead of silently dropping it.
  You decide what happens next: locate the new folder, retry, or remove the project from Arbor.
</p>

<h2>How it works</h2>

<p>At workspace restore time and on every "Open project" attempt, Arbor classifies the path into
one of four states:</p>

<table class="shortcuts-table">
  <thead><tr><th>Status</th><th>Meaning</th><th>Typical cause</th></tr></thead>
  <tbody>
    <tr><td><code>ok</code></td><td>Path exists and is a valid git repo</td><td>Normal case</td></tr>
    <tr><td><code>missing</code></td><td>Path doesn't exist, but at least one ancestor does</td><td>Folder deleted or moved</td></tr>
    <tr><td><code>unreachable</code></td><td>Neither the path nor any ancestor can be stat-ed</td><td>Drive unmounted, network share offline, VPN disconnected</td></tr>
    <tr><td><code>not_a_repo</code></td><td>Path exists but is not a git repo</td><td><code>.git/</code> deleted or repo moved out</td></tr>
  </tbody>
</table>

<p>Anything other than <code>ok</code> places the tab into <strong>tombstone</strong> state — the tab still appears in the
title bar with a warning glyph, and clicking it opens the locate UI instead of trying to read git2.</p>

<h2>The tombstone screen</h2>

<p>When a tombstoned tab is active, the main area shows the missing-project panel. Available actions:</p>

<ul class="prop-list">
  <li><strong>Locate folder…</strong>Pick the new on-disk location for the project. Arbor validates it as a git repo, updates the registry, refreshes recents, and reopens the tab in place.</li>
  <li><strong>Retry</strong>Re-classify the original path. Useful after remounting a drive or reconnecting to a VPN.</li>
  <li><strong>Remove from Arbor</strong>Deregister the project: removes it from every workspace and clears its registry entry. The folder on disk is never touched.</li>
</ul>

<Callout variant="info" title="Re-validate on focus">
  By default, Arbor re-classifies every tombstoned tab when the window regains focus, so a tab can
  return to a normal repo automatically once you remount the drive. You can turn this off in
  <strong>Settings → Git → Missing Projects</strong>.
</Callout>

<h2>Recent projects (Welcome screen)</h2>

<p>The "Recent" and workspace-repo lists on the welcome screen are validated in parallel on load.
Missing entries are shown with:</p>

<ul>
  <li>A warning glyph and strikethrough name</li>
  <li>A <code>missing</code> badge</li>
  <li>Inline <strong>Locate</strong> and <strong>Remove</strong> buttons (recents) or just <strong>Locate</strong> (workspace repos)</li>
</ul>

<p>Clicking a missing row never tries to open it — it goes straight to the locate picker.
You can also bulk-clean every dead recent in <strong>Settings → Git → Missing Projects → Clean up missing recents</strong>.</p>

<h2>Settings</h2>

<table class="shortcuts-table">
  <thead><tr><th>Setting</th><th>Default</th><th>Effect</th></tr></thead>
  <tbody>
    <tr>
      <td><code>auto_prune_recents</code></td>
      <td>off</td>
      <td>Silently drop missing entries from the Recent list at load time. When off, they're shown with the missing badge so you can act per-entry.</td>
    </tr>
    <tr>
      <td><code>confirm_before_remove</code></td>
      <td>on</td>
      <td>Require a second click on the tombstone screen's "Remove" button before deregistering.</td>
    </tr>
    <tr>
      <td><code>revalidate_on_focus</code></td>
      <td>on</td>
      <td>Re-classify tombstoned tabs whenever the app regains focus.</td>
    </tr>
  </tbody>
</table>

<h2>Plugin hooks</h2>

<p>Two hooks bracket the tombstone lifecycle. Both fire with a single context table.</p>

<ul class="prop-list">
  <li><code>on_project_missing</code>Fired when a registered repo's path fails validation at open time. Plugins should drop transient state tied to that project (cancel jobs, hide pinned views) but should NOT delete persistent caches — the user might recover the path.</li>
  <li><code>on_project_relocated</code>Fired after the user picks a new location via the Locate flow. Plugins keyed off the absolute path (deps caches, IDE history, …) should rebase their bookkeeping from <code>old_path</code> to <code>new_path</code>.</li>
</ul>

<h3>Context tables</h3>

<pre><code>{`-- on_project_missing
{
  repo_id = "uuid…",
  path    = "/old/path",
  name    = "myrepo",       -- nil if no longer in registry
  reason  = "missing" | "unreachable" | "not_a_repo",
}

-- on_project_relocated
{
  repo_id    = "uuid…",
  old_path   = "/old/path",
  new_path   = "/new/path",
  name       = "myrepo",
  remote_url = "git@…" or nil,
}`}</code></pre>

<h3>Example handler</h3>

<pre><code>{`arbor.events.on("on_project_relocated", function(ctx)
  -- Rewrite our path-keyed cache
  local cache = arbor.settings.global.get("path_cache") or {}
  if cache[ctx.old_path] then
    cache[ctx.new_path] = cache[ctx.old_path]
    cache[ctx.old_path] = nil
    arbor.settings.global.set("path_cache", cache)
  end
end)

arbor.events.on("on_project_missing", function(ctx)
  arbor.log.warn("project missing: " .. ctx.path .. " (" .. ctx.reason .. ")")
end)`}</code></pre>

<div class="hint">
  Both hooks fire from the backend with the same dispatch pipeline as
  <code>on_repo_open</code> / <code>on_repo_close</code>, so anything you can do from those handlers
  works here.
</div>

<h2>Distinguishing missing from drive-offline</h2>

<p>The <code>reason</code> field lets plugins behave differently for "drive disconnected" vs.
"folder deleted":</p>

<ul>
  <li><code>missing</code> usually means the user moved the folder. A plugin might choose to remove its persistent state for that project, since the path is unlikely to come back.</li>
  <li><code>unreachable</code> usually means the user is on a flaky network. Plugins should keep state and let the next focus revalidation pick up where they left off.</li>
  <li><code>not_a_repo</code> means the directory is still there but the <code>.git/</code> is gone. The user may be restoring from backup; treat it like <code>missing</code> but more transient.</li>
</ul>
