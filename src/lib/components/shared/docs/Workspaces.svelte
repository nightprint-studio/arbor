<h1>Workspaces</h1>

<p class="doc-lead">
  A <strong>workspace</strong> is a named, colour-coded group of repositories.
  Switching workspace replaces the entire tab set with whatever was open the last
  time you were there, so you can jump between unrelated projects without
  losing context.
</p>

<div class="callout tip">
  <strong>Workspace dropdown</strong>
  The dropdown in the top bar (next to the hamburger menu) shows the active
  workspace and lets you switch between them. Every installation has a
  built-in <strong>Scratch</strong> workspace that collects ad-hoc opens.
</div>

<h2>Key concepts</h2>
<div class="feature-grid two-col">
  <div class="feature-card">
    <div class="fc-title">Central registry</div>
    <div class="fc-desc">Every repository Arbor has ever seen lives in <code>~/.config/arbor/repos.json</code> with a stable UUID. Workspaces reference that UUID, so renaming or relocating a repo is a one-place edit.</div>
  </div>
  <div class="feature-card">
    <div class="fc-title">Membership is many-to-many</div>
    <div class="fc-desc">A repo can belong to several workspaces at once — membership is just a reference. Removing it from one workspace never deletes the repo or its path on disk.</div>
  </div>
  <div class="feature-card">
    <div class="fc-title">Tab snapshots</div>
    <div class="fc-desc">Each workspace has its own tab snapshot (<code>workspace-state/&lt;uuid&gt;.json</code>). Switching saves the current snapshot and restores the target one. Panel sizes remain global.</div>
  </div>
  <div class="feature-card">
    <div class="fc-title">Scratch</div>
    <div class="fc-desc">Non-deletable fall-back. Every ad-hoc opened repo lands here until you move it to a named workspace.</div>
  </div>
</div>

<h2>Creating and managing workspaces</h2>
<ol class="step-list">
  <li>Click the workspace dropdown → <strong>Manage Workspaces…</strong> (or use your keybinding).</li>
  <li>Hit <strong>+ New Workspace</strong>. Give it a name, a palette colour, an optional group, and tick the repos that should belong to it.</li>
  <li>Use the management modal to rename, reorder, move repos between workspaces, or delete workspaces you don't need any more. Deleting a workspace never removes the repos themselves.</li>
</ol>

<p>
  Inside the management modal, <strong>Arrow Up / Down</strong> navigates
  groups, workspaces and (when expanded) repo rows; <strong>Space</strong>
  expands or collapses the focused group or workspace; <strong>Enter</strong>
  on a repo row opens it. <strong>Arrow Down</strong> from the search box
  drops focus into the list.
</p>

<h2>Groups (optional)</h2>
<p>
  Groups are a purely visual organisation aid — they let you nest several
  workspaces under a single collapsible header (handy for client/project/team
  separation). Creating a group from the management modal adds a header that
  you can drag workspaces into. Deleting a group orphans its children back to
  the top level; it never cascades through to the workspaces themselves.
</p>

<h2>Cross-workspace tabs</h2>
<p>
  Opening a repo that belongs to a different workspace opens it as a
  <strong>cross-workspace tab</strong>: a small coloured dot (with the source
  workspace's initials) appears on the tab, and the tab is marked with a
  dashed left border. Cross-WS tabs live only in the current workspace's
  snapshot — they are not added to its membership. Right-click a cross-WS
  tab to <strong>Add to active workspace</strong> and flip it into a regular
  member.
</p>

<h2>Import / export</h2>
<p>
  <strong>Export</strong> a workspace from the management modal: Arbor
  copies a portable JSON blob to the clipboard containing the workspace
  name, colour, and each member's <em>display name and remote URL</em>
  (paths are intentionally omitted so the file works across machines).
</p>
<p>
  <strong>Import</strong> takes that JSON and shows a preview table. For
  each repo the row proposes an action:
</p>
<ul>
  <li><strong>Use existing</strong> — if Arbor already has a matching repo (matched by remote URL) locally.</li>
  <li><strong>Locate…</strong> — pick a folder on disk where the repo already lives.</li>
  <li><strong>Clone…</strong> — type a destination path; Arbor shells out to <code>git clone</code>.</li>
  <li><strong>Skip</strong> — leave this one out of the imported workspace.</li>
</ul>
<p>The <strong>Create Workspace</strong> button stays disabled until every row is resolved.</p>

<h2>Bulk operations</h2>
<p>
  Each workspace header carries a small toolbar of bulk actions. They all
  share the same engine: a single aggregated background job that walks the
  workspace's repos sequentially, logging per-repo progress to the Job Output
  panel. Individual repo rows show a spinner while queued and flip back to a
  branch / ahead-behind chip when their step completes.
</p>
<table class="shortcuts-table">
  <thead><tr><th>Action</th><th>What it does</th></tr></thead>
  <tbody>
    <tr>
      <td><strong>Fetch all</strong></td>
      <td>Runs <code>git fetch</code> on every member's preferred remote (origin first, then the first one configured). Never modifies the workdir.</td>
    </tr>
    <tr>
      <td><strong>Pull all</strong></td>
      <td>Fetch + fast-forward / merge per member. Skips repos in detached HEAD; surfaces conflicts with a distinct row badge so you know which projects need attention.</td>
    </tr>
    <tr>
      <td><strong>Tag all</strong></td>
      <td>Opens a modal to create the same tag (lightweight or annotated) at the current HEAD of every member, with optional push. See below.</td>
    </tr>
  </tbody>
</table>

<h2>Tag all (release)</h2>
<p>
  The tag icon on the workspace header opens a pre-flight modal that scans
  every member and surfaces any conditions you'd want to know about
  <em>before</em> stamping a release tag across the whole group:
</p>
<ul>
  <li><strong>Detached HEAD</strong> — the repo is not on a branch; it will be skipped (a tag at a parked commit is almost always a mistake).</li>
  <li><strong>Behind upstream</strong> — the local branch hasn't pulled yet; the tag would land on a stale commit.</li>
  <li><strong>Local modifications</strong> — uncommitted changes in the workdir; the tag would point at the last commit, ignoring your work-in-progress.</li>
  <li><strong>Merge in progress</strong> — an unfinished merge / rebase / cherry-pick / revert; resolve before tagging.</li>
  <li><strong>Path missing</strong> — the repo on disk has been moved or deleted; it is skipped.</li>
</ul>
<p>
  Type a tag name and an optional message — when the message is non-empty the
  tag becomes annotated, otherwise it's lightweight. The footer carries a
  <strong>split button</strong> with two modes:
</p>
<ul>
  <li><strong>Create tag</strong> — creates the tag locally on every member.</li>
  <li><strong>Create tag &amp; push</strong> — creates the tag, then pushes <code>refs/tags/&lt;name&gt;</code> to each member's preferred remote.</li>
</ul>

<h2>Worktrees and workspace membership</h2>
<p>
  A workspace lists <strong>root repositories</strong>, not the individual
  worktree paths underneath them. The picker in the create / edit modal
  intentionally hides linked worktrees: adding both the root and a worktree
  of the same project would be the same project listed twice.
</p>
<ul>
  <li><strong>Add the root once</strong> — pick the main checkout from the create / edit modal.</li>
  <li><strong>Switch to a worktree from inside the tab</strong> — open the worktrees sidebar and click <em>Switch</em> on a sibling. The active tab swaps its working path; the workspace membership stays put.</li>
  <li><strong>Indicators</strong> — a member that's currently sitting on a linked worktree shows a small worktree icon next to its branch label, and the workspace header gets a tiny worktree badge so the information stays visible while collapsed.</li>
  <li><strong>Legacy members</strong> — workspaces created before this change may already include a worktree path as a member. The edit modal still shows it (with a <code>worktree</code> tag and a softer style) so you can deselect it; new pickers won't offer worktrees again.</li>
</ul>

<h2>Startup behaviour</h2>
<ul>
  <li>Arbor auto-restores the last active workspace, including its open tabs and active tab.</li>
  <li>When launching for the first time after upgrading, any repos from the legacy session are migrated into Scratch and the welcome screen offers to organise them.</li>
  <li>Auto-switch to Scratch happens if the active workspace is deleted while it is active.</li>
</ul>

<h2>Command Palette</h2>
<table class="shortcuts-table">
  <thead><tr><th>Shortcut</th><th>Verb</th><th>Description</th></tr></thead>
  <tbody>
    <tr><td><kbd>Ctrl+N</kbd></td><td>Open Project</td><td>Fuzzy-open any repo from the active workspace (plus Scratch).</td></tr>
    <tr><td><kbd>Ctrl+Shift+N</kbd></td><td>Open from Workspace</td><td>Fuzzy-open a repo from any <em>other</em> workspace as a cross-workspace tab.</td></tr>
    <tr><td><kbd>Ctrl+K</kbd></td><td>Switch Workspace</td><td>Type <em>switch workspace</em> and pick a target. Saves the current snapshot, restores the target's.</td></tr>
  </tbody>
</table>

<div class="callout info">
  <strong>Not to be confused with…</strong>
  Git <em>worktrees</em> are an unrelated feature that share history but check
  out different branches — see the <em>Worktrees</em> documentation. Arbor
  workspaces are a UI-level grouping with no git-level counterpart.
</div>
