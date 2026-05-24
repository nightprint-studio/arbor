<script lang="ts">
  import Callout from '$lib/components/shared/ui/Callout.svelte';
</script>

<h1>Submodules</h1>

<p class="doc-lead">Arbor shows rich status for each Git submodule — current branch, ahead/behind counts, dirty state — and lets you fetch, pull, push, and switch branches directly from the sidebar.</p>

<h2>Sidebar section</h2>
<p>When the active repository contains submodules, a <strong>Submodules</strong> section appears in the Branches &amp; Stashes sidebar (below Tags). It is hidden automatically for repos with no submodules.</p>
<p>The section badge turns amber when at least one submodule is uninitialised, has local changes, or is behind its remote tracking branch.</p>

<h2>Row layout</h2>
<p>Each row shows two lines on the left and a set of badges on the right:</p>

<table class="shortcuts-table">
  <thead><tr><th>Element</th><th>Description</th></tr></thead>
  <tbody>
    <tr><td><strong>Name</strong></td><td>Submodule name in primary text. A <span style="color:#e2a335">•</span> dot indicates a dirty working directory (uncommitted changes inside the submodule).</td></tr>
    <tr><td><strong>Path</strong></td><td>Relative path from the parent repo root, shown in a smaller monospace font.</td></tr>
    <tr><td>Branch badge</td><td>Pill showing the current branch name. If the submodule is in detached HEAD state, the short commit hash is shown in an amber badge instead.</td></tr>
    <tr><td><span style="color:var(--success,#5fb760)">↑N</span> Ahead</td><td>Number of commits the submodule is ahead of its remote tracking branch (green).</td></tr>
    <tr><td><span style="color:#e2a335">↓N</span> Behind</td><td>Number of commits the submodule is behind its remote tracking branch (amber).</td></tr>
    <tr><td><span style="color:var(--success,#5fb760)">●</span> Synced</td><td>Small green dot — visible only when the submodule has a branch and is fully in sync (ahead = 0, behind = 0).</td></tr>
    <tr><td>⚠ Warning icon</td><td>Shown when the submodule is not initialised / not cloned yet.</td></tr>
    <tr><td>Spinner</td><td>Replaces all badges while a fetch / pull / push is in progress for that row.</td></tr>
  </tbody>
</table>

<h2>Opening a submodule as a tab</h2>
<p>An initialised submodule is itself a full Git repository. You can open it in its own tab in two ways:</p>
<ul>
  <li><strong>Double-click</strong> the row.</li>
  <li>Right-click → <strong>Open as Tab</strong> from the context menu.</li>
</ul>
<p>Arbor checks whether the path is already open; if so it switches to the existing tab instead of opening a duplicate.</p>

<h2>Context menu operations</h2>
<p>Right-click any row to open the context menu.</p>

<table class="shortcuts-table">
  <thead><tr><th>Action</th><th>Git equivalent</th></tr></thead>
  <tbody>
    <tr><td><strong>Fetch</strong></td><td><code>git fetch</code> inside the submodule directory</td></tr>
    <tr><td><strong>Pull</strong></td><td><code>git pull</code> inside the submodule directory</td></tr>
    <tr><td><strong>Push</strong></td><td><code>git push</code> inside the submodule directory</td></tr>
    <tr><td><strong>Checkout Branch…</strong></td><td>Opens the <em>Checkout Branch</em> modal (see below)</td></tr>
    <tr><td><strong>Open as Tab</strong></td><td>Opens the submodule as a new tab in Arbor</td></tr>
  </tbody>
</table>

<p>All sync operations (Fetch / Pull / Push) are disabled for uninitialised submodules. After each operation the sidebar data refreshes automatically. Errors (e.g. merge conflicts on pull, rejected push) are shown as toast notifications containing the raw <code>git</code> output.</p>

<h2>Checkout Branch modal</h2>
<p>Select <strong>Checkout Branch…</strong> from the context menu to open a compact modal that:</p>
<ul>
  <li>Lists all local and remote branches available in the submodule (remote branches have their <code>origin/</code> prefix stripped and are deduplicated).</li>
  <li>Pre-selects and marks the currently checked-out branch as <em>current</em>.</li>
  <li>Disables the Confirm button when the current branch is already selected.</li>
  <li>Shows a spinner during the branch-list fetch and during the checkout itself.</li>
</ul>

<Callout variant="info" title="Adding or removing submodules">
  These operations are not supported from the UI. Use the integrated terminal or an external shell:
  <code>git submodule add &lt;url&gt; &lt;path&gt;</code> / <code>git rm &lt;path&gt;</code>
</Callout>

<h2>Initialising submodules</h2>
<p>Uninitialised (not-yet-cloned) submodules show a warning icon and all sync operations are disabled. To initialise them use the integrated terminal:</p>
<pre><code>git submodule update --init
# or, for nested submodules:
git submodule update --init --recursive</code></pre>

<h2>Technical notes</h2>
<ul>
  <li>All submodule operations spawn a <code>git</code> CLI subprocess with the submodule directory as the working directory — they do not use libgit2's submodule write API, which is incomplete.</li>
  <li>Ahead/behind counts are computed with <code>git2::Repository::graph_ahead_behind()</code> against the upstream tracking branch configured inside the submodule's own <code>.git/config</code>. If no upstream is configured the counts show as 0.</li>
</ul>
