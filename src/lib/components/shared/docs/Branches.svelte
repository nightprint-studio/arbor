<script lang="ts">
  import Callout from '$lib/components/shared/ui/Callout.svelte';
  import Kbd     from '$lib/components/shared/internal/Kbd.svelte';
</script>

<h1>Branch Management</h1>

<p class="doc-lead">Create, switch, rename, and clean up branches without leaving the UI. Ahead/behind counts refresh in real time after every fetch.</p>

<h2>Creating &amp; checking out</h2>
<div class="feature-grid">
  <div class="feature-card">
    <div class="fc-title">Create from a commit</div>
    <div class="fc-desc">Right-click any commit in the graph → <strong>Create Branch</strong>. Checked out immediately.</div>
  </div>
  <div class="feature-card">
    <div class="fc-title">Double-click</div>
    <div class="fc-desc">Double-click a branch row in the sidebar to check it out.</div>
  </div>
  <div class="feature-card">
    <div class="fc-title">Right-click menu</div>
    <div class="fc-desc">Right-click any branch → <strong>Checkout</strong>.</div>
  </div>
  <div class="feature-card">
    <div class="fc-title">Command Palette</div>
    <div class="fc-desc">Press <Kbd action="command_palette" /> and type the branch name for a fuzzy match.</div>
  </div>
</div>

<h2>Drag to merge or compare</h2>
<p>Drag any branch from the sidebar onto another branch — both local and remote branches are draggable.</p>
<ol class="step-list">
  <li>Hold and drag a branch row — a floating label follows your cursor</li>
  <li>Drop onto another branch — the target row highlights with a dashed border</li>
  <li>A small context menu appears with the available actions</li>
</ol>

<h3>Available actions</h3>
<p>When the drop target is the <strong>current HEAD</strong> (local → local), the menu offers four merge strategies plus Compare:</p>
<dl class="meta-grid">
  <dt>Merge</dt><dd>Default <code>git merge</code> — fast-forward when possible, otherwise a merge commit.</dd>
  <dt>Merge (no fast-forward)</dt><dd>Always creates a merge commit, even when the history is linear. Equivalent to <code>git merge --no-ff</code>.</dd>
  <dt>Squash merge</dt><dd>Combines all commits of the source into the index without committing — <em>review &amp; commit from the Stage panel</em> when done.</dd>
  <dt>Fast-forward only</dt><dd>Refuses to create a merge commit. Errors out (and offers no rewrite) when the source isn't a strict descendant of the target.</dd>
  <dt>Compare</dt><dd>Full diff modal between the two tips. Always available — works for any local/remote combination.</dd>
</dl>

<h3>From the Command Palette</h3>
<p>The same four strategies are reachable without drag-and-drop. Press <Kbd action="command_palette" /> and type one of the merge verbs, then pick a branch as the target — HEAD is always the recipient:</p>
<dl class="meta-grid">
  <dt><code>Merge</code></dt><dd>Default strategy — fast-forward when possible, otherwise a merge commit. Aliases: <code>merge-default</code>.</dd>
  <dt><code>Merge (no fast-forward)</code></dt><dd>Always produces a merge commit. Aliases: <code>no-ff</code>, <code>noff</code>.</dd>
  <dt><code>Squash Merge</code></dt><dd>Stages the combined diff without committing. Alias: <code>squash</code>.</dd>
  <dt><code>Fast-forward Only</code></dt><dd>Advances HEAD only when a strict fast-forward is possible. Aliases: <code>ff</code>, <code>ff-only</code>.</dd>
</dl>
<p>Outcome toasts mirror the drag-and-drop flow, including the conflict warning that redirects you to the Stage panel.</p>

<h4>Merge outcome toasts</h4>
<dl class="meta-grid">
  <dt><code>already_up_to_date</code></dt><dd>"<em>target</em> already contains <em>source</em> — nothing to merge". No commit created.</dd>
  <dt><code>fast_forward</code></dt><dd>Plain fast-forward — branch tip advanced, no merge commit.</dd>
  <dt><code>merged</code></dt><dd>Merge commit was written.</dd>
  <dt><code>squashed</code></dt><dd>Changes staged but not committed — Stage panel takes over.</dd>
</dl>

<Callout variant="info" title="Merge direction">
  Dragging <code>feature</code> onto <code>main</code> merges <em>feature into main</em>, not the reverse. The target (drop target) is always the recipient.
</Callout>

<h3>Compare modal</h3>
<p>Left panel lists all files that differ between the two tips; click one to load its diff on the right with full syntax highlighting and unified/split mode. Identical branches show a notice instead of an empty list.</p>

<h3>Merge with conflicts</h3>
<p>If the merge can't complete automatically, Arbor runs it as far as possible and leaves the repo in a mid-merge state. A warning toast guides you to the <strong>Stage</strong> panel where the conflict resolver takes over.</p>

<h2>Remote operations</h2>
<div class="feature-grid">
  <div class="feature-card">
    <div class="fc-title">Fetch</div>
    <div class="fc-desc">Download remote refs without merging. Status-bar button or sidebar.</div>
  </div>
  <div class="feature-card">
    <div class="fc-title">Pull</div>
    <div class="fc-desc">Fetch + fast-forward (or merge) the current branch. Stashes dirty changes automatically if needed.</div>
  </div>
  <div class="feature-card">
    <div class="fc-title">Push</div>
    <div class="fc-desc">Push the current branch to <code>origin</code>. Right-click HEAD in the graph → <strong>Push</strong>.</div>
  </div>
</div>
<p>The sidebar shows ahead/behind counts as <strong>▲ N</strong> (unpushed) and <strong>▼ N</strong> (behind remote) indicators on each branch. Local branches with no upstream tracking ref get a purple <strong>local</strong> badge so it's obvious which branches still need a first push.</p>

<p>The status bar at the bottom shows the current branch as a clickable chip — <strong>click it to copy the branch name</strong> to the clipboard.</p>

<h2>Renaming a branch</h2>
<p>Right-click any local branch → <strong>Rename…</strong>. A modal opens pre-filled with the current name.</p>

<h3>Name rules</h3>
<ul class="prop-list">
  <li><strong>Non-empty</strong>Cannot equal the current name.</li>
  <li><strong>No leading</strong>Cannot start with <code>-</code> or <code>.</code></li>
  <li><strong>Forbidden</strong>No spaces, no <code>~ ^ : ? * [ \</code></li>
  <li><strong>Sequences</strong>No <code>..</code>, no trailing <code>.</code> / <code>/</code>, no <code>@&#123;&#125;</code></li>
</ul>

<h3>Also rename the remote branch</h3>
<p>If a remote tracking ref exists, a toggle <strong>"Also rename remote branch"</strong> appears. Enabled, Arbor runs:</p>
<ol class="step-list">
  <li>Rename the local branch</li>
  <li>Push the new name to the remote (<code>git push &lt;remote&gt; &lt;new-name&gt;</code>)</li>
  <li>Delete the old remote branch (<code>git push &lt;remote&gt; --delete &lt;old-name&gt;</code>)</li>
</ol>
<Callout variant="danger" title="Irreversible — remote rename">
  Once the old remote branch is deleted, any teammate tracking it will have a broken upstream. The rename button turns red and shows <strong>"Rename + Delete Remote"</strong> as a confirmation prompt.
</Callout>
<Callout variant="warning" title="After a local-only rename">
  Without the remote toggle, only the local ref updates. Update the upstream manually:<br>
  <code>git branch --set-upstream-to=&lt;remote&gt;/&lt;new-name&gt; &lt;new-name&gt;</code>
</Callout>

<h3>Renaming a remote-only branch</h3>
<p>Right-click any <code>origin/&lt;branch&gt;</code> row → <strong>Rename…</strong> to open a dedicated <em>Remote Branch Rename</em> modal. The flow is three steps and is shown progressively as it runs (push tip → delete old → optional local rename):</p>
<ol class="step-list">
  <li><strong>Push</strong> the existing remote tip to the new name (<code>git push &lt;remote&gt; &lt;old-sha&gt;:refs/heads/&lt;new-name&gt;</code>).</li>
  <li><strong>Delete</strong> the old remote ref (<code>git push &lt;remote&gt; --delete &lt;old-name&gt;</code>).</li>
  <li>If a <strong>local branch with the same short name</strong> exists, an <em>"Also rename my local branch"</em> toggle (on by default) renames it and re-points its upstream to the new remote ref. Otherwise the toggle is hidden.</li>
</ol>
<p>The same name-validation rules as the local rename apply (no spaces, forbidden chars, <code>..</code>, leading <code>-</code>/<code>.</code>, etc.).</p>
<Callout variant="danger" title="Irreversible">
  Teammates tracking the old name will have a broken upstream once step 2 lands. The confirm button is red and labelled <em>"Rename + Delete Remote"</em>.
</Callout>

<h2>Deleting branches</h2>
<dl class="meta-grid">
  <dt>Local</dt><dd>Right-click any local branch → <strong>Delete Branch</strong>. Current HEAD cannot be deleted.</dd>
  <dt>Remote</dt><dd>Right-click any remote branch → <strong>Delete remote branch</strong>. Confirmation modal appears first.</dd>
  <dt>Bulk</dt><dd>Use <strong>Branch Cleanup</strong> (trash icon in <em>Local Branches</em> header).</dd>
</dl>
<Callout variant="danger" title="Irreversible — pushes a delete">
  Deleting a remote branch runs <code>git push origin --delete &lt;branch&gt;</code>. Any teammate with a tracking ref will have a broken upstream. Requires credentials configured in <em>Settings → Git &amp; Integrations</em>.
</Callout>

<h2>Branch Cleanup</h2>
<p>The <strong>trash icon</strong> in the sidebar's <em>Local Branches</em> header opens the Branch Cleanup modal. It scans for branches whose tip is fully reachable from a target branch (already merged).</p>

<div class="feature-grid two-col">
  <div class="feature-card">
    <div class="fc-title">Local tab</div>
    <div class="fc-desc">Click <strong>Scan</strong> — all merged branches pre-selected. Deselect to keep any, then bulk-delete.</div>
  </div>
  <div class="feature-card">
    <div class="fc-title">Remote tab</div>
    <div class="fc-desc">Loads on open. Deletes push <code>--delete</code> refspec and remove the local tracking ref.</div>
  </div>
</div>
<p>Both tabs share the same target selector — defaults to the current HEAD (or <code>main</code> / <code>master</code> as fallback).</p>

<h2>Rebase</h2>
<p>Available via the graph context menu on the target commit. Arbor delegates rebase to the <code>git</code> CLI since the libgit2 API doesn't support interactive rebase.</p>
