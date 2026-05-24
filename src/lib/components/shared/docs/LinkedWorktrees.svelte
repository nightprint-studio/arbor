<script lang="ts">
  import Callout from '$lib/components/shared/ui/Callout.svelte';
</script>

<h1>Linked Worktrees</h1>

<Callout variant="warning" title="Experimental feature">
  Linked Worktrees is still being shaped — file format
  (<code>linked_worktrees.toml</code>), Tauri commands, plugin hooks and the
  edge-case behaviour around stash conflicts may change between releases.
  Avoid building plugins or muscle memory around the wire-format details for
  now; the user-facing UI (manager modal, badges, command palette verbs) is
  the stable surface.
</Callout>

<p class="doc-lead">
  A <strong>worktree link</strong> ties several worktrees together so that a
  branch checkout on one member is propagated to the others.  Links are
  optional and orthogonal to workspaces: a workspace groups repos for
  navigation, a link coordinates their HEADs.
</p>

<Callout variant="tip" title="Where it lives">
  Persisted in <code>~/.config/arbor/linked_worktrees.toml</code>.  Members
  are identified by their <code>RepoRegistry</code> UUID (the same identity
  used by workspaces, keyed by path), so each worktree path is its own
  member — multiple worktrees of the same repo are independent links.
</Callout>

<h2>How sync works</h2>
<ol class="step-list">
  <li>You check out a branch on a tab whose worktree is a member of a sync-enabled link.</li>
  <li>The local checkout completes first.  If it fails, no propagation runs.</li>
  <li>Arbor iterates the other members in a background thread, serialised: stash dirty workdir → checkout the resolved branch → stash apply.</li>
  <li>Members where the branch is missing are <em>skipped silently</em>.  An aggregated notification at the end summarises updated / skipped / conflicting members.</li>
</ol>

<Callout variant="info" title="Stash safety">
  Pre-checkout stashes use <code>git stash push -u</code> so untracked files
  are preserved.  Re-application uses <code>git stash apply</code>, not
  <code>pop</code>: a clean apply drops the entry, but on conflict the entry
  is kept so nothing is silently lost.
</Callout>

<h2>Branch aliases</h2>
<p>
  When the same logical branch has different names per repo
  (<code>feature/X</code> on repo&nbsp;A vs <code>feature/Y</code> on repo&nbsp;B),
  declare an <strong>alias group</strong> in the link.  An alias group is an
  equivalence class — a set of <code>(repo_id, branch)</code> pairs that all
  resolve to "the same branch" during sync.
</p>
<ul>
  <li><strong>Resolution rule</strong> — when checking out, Arbor looks for an alias group containing the initiator's <code>(repo_id, branch)</code>.  If found, every other member uses the branch declared in that group.  Members not present in the group fall back to identity-name.</li>
  <li><strong>Alias wins over identity</strong> — if the alias group says repo&nbsp;B should use <code>feature/Y</code> but <code>feature/X</code> also exists on B, the checkout still goes to <code>feature/Y</code>.</li>
  <li><strong>Smart cleanup</strong> — deleting a branch removes any alias entries pointing to it; renaming a branch updates the entries; if all entries in a group end up sharing the same name, the group is dropped automatically.</li>
  <li><strong>Create-branch guard</strong> — creating a branch whose name is reserved by an alias on this repo is refused with a message pointing at the offending link.</li>
</ul>

<h2>Managing links</h2>
<ol class="step-list">
  <li>Open the manager from <strong>Ctrl+K → Manage Linked Worktrees</strong> or click the link badge in the graph toolbar.</li>
  <li>Create a new link; add members from the registry.  A worktree can belong to at most one link.</li>
  <li>Toggle <strong>Sync enabled</strong> per link to pause propagation temporarily — checkouts won't propagate while disabled, and re-enabling does <em>not</em> auto-resync (the next checkout will).</li>
  <li>Add or edit alias groups; pick at least 2 members per group.</li>
</ol>

<h2>Out-of-sync indicator</h2>
<p>
  Once a link has performed at least one sync, the graph toolbar shows a
  <strong>link badge</strong> whenever the active tab belongs to that link.
  An amber dot lights up when the tab's current branch differs from the
  expected one (i.e., it's out of sync with the link's last target).
  Clicking the badge opens the manager pre-selected on that link.
</p>

<h2>Edge cases</h2>
<table class="shortcuts-table">
  <thead><tr><th>Situation</th><th>Behaviour</th></tr></thead>
  <tbody>
    <tr><td>Branch missing on a member</td><td>Skipped; counted in the aggregated notification.</td></tr>
    <tr><td>Member is in detached HEAD</td><td>Stashed + checked out like any other member.</td></tr>
    <tr><td>Member's path is missing</td><td>Skipped with a "broken member" reason; visible in the manager.</td></tr>
    <tr><td>Member already on the target branch</td><td>Skipped silently — no work to do.</td></tr>
    <tr><td>Member is not currently open as a tab</td><td>Repo is opened in the background just for the checkout, no UI tab is added.</td></tr>
    <tr><td>Concurrent sync on the same link</td><td>Serialised by an in-progress guard; recursive triggers are suppressed.</td></tr>
    <tr><td>Tab swaps to a different worktree (e.g. via the Worktrees sidebar)</td><td>The badge follows the new path.  If the new worktree is not in any link, the badge disappears.</td></tr>
    <tr><td>Checkout from the integrated terminal</td><td>Not intercepted — only checkouts via the Arbor UI / Lua API propagate.</td></tr>
  </tbody>
</table>

<h2>Plugin API</h2>
<p>
  Plugins can introspect links and toggle sync via the
  <code>arbor.linked_worktrees</code> table.  No create/delete operations
  are exposed — those are user-only.
</p>
<table class="shortcuts-table">
  <thead><tr><th>Function</th><th>Returns</th><th>Notes</th></tr></thead>
  <tbody>
    <tr><td><code>arbor.linked_worktrees.list()</code></td><td>array of <code>&#123;id, name, sync_enabled, member_count&#125;</code></td><td>Sorted by name.</td></tr>
    <tr><td><code>arbor.linked_worktrees.get(id)</code></td><td>full <code>WorktreeLink</code> table or nil</td><td>Includes members + alias groups.</td></tr>
    <tr><td><code>arbor.linked_worktrees.set_sync_enabled(id, enabled)</code></td><td>bool (true on success)</td><td>Persisted immediately.</td></tr>
  </tbody>
</table>

<h2>Hooks</h2>
<table class="shortcuts-table">
  <thead><tr><th>Hook</th><th>Context</th><th>Fires when</th></tr></thead>
  <tbody>
    <tr><td><code>"on_worktree_link_sync_started"</code></td><td><code>&#123;link_id, link_name, initiator_repo_id, target_branch&#125;</code></td><td>Just before propagation begins.</td></tr>
    <tr><td><code>"on_worktree_link_sync_done"</code></td><td><code>&#123;link_id, link_name, target_branch, results: [...]&#125;</code></td><td>After every member has been processed.  Each result has <code>repo_id</code> and a <code>status</code> table tagged by <code>kind</code>.</td></tr>
    <tr><td><code>"on_worktree_link_member_added"</code></td><td><code>&#123;link_id, repo_id&#125;</code></td><td>User added a worktree to a link.</td></tr>
    <tr><td><code>"on_worktree_link_member_removed"</code></td><td><code>&#123;link_id, repo_id&#125;</code></td><td>User removed a worktree from a link.</td></tr>
  </tbody>
</table>

<h2>Command Palette</h2>
<table class="shortcuts-table">
  <thead><tr><th>Verb</th><th>Description</th></tr></thead>
  <tbody>
    <tr><td>Manage Linked Worktrees</td><td>Opens the manager modal.</td></tr>
    <tr><td>Link this Worktree…</td><td>Opens a picker — pick an existing link or create a new one with this worktree as the first member.</td></tr>
    <tr><td>Unlink from "&lt;name&gt;"</td><td>Removes the current worktree from its link (after confirmation).</td></tr>
    <tr><td>Enable/Disable Sync for "&lt;name&gt;"</td><td>Toggles sync on the active tab's link.</td></tr>
  </tbody>
</table>
