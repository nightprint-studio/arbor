<h1>Git Graph</h1>

<p class="doc-lead">The commit graph renders your entire repository history as SVG lanes with virtual scrolling — only visible rows are painted, regardless of repository size or branch count.</p>

<h2>Navigation</h2>
<table class="shortcuts-table">
  <thead><tr><th>Action</th><th>How</th></tr></thead>
  <tbody>
    <tr><td>Select commit &amp; load diff</td><td>Click any row</td></tr>
    <tr><td>Context menu</td><td>Right-click any row</td></tr>
    <tr><td>Load more history</td><td>Scroll to the bottom — loads automatically (when pagination is on)</td></tr>
    <tr><td>Search commits</td><td><kbd>Ctrl+F</kbd> — message, author name, or SHA</td></tr>
    <tr><td>Next search match</td><td><kbd>Enter</kbd> while search is open (or the ▼ button)</td></tr>
    <tr><td>Previous search match</td><td><kbd>Shift+Enter</kbd> while search is open (or the ▲ button)</td></tr>
    <tr><td>Jump to HEAD</td><td><kbd>Ctrl+Home</kbd> or the ↑ button in the toolbar</td></tr>
  </tbody>
</table>

<h2>Commit node indicators</h2>
<ul class="indicator-list">
  <li><span class="ind ind-bright"></span><span><strong>Avatar circle</strong> — each regular commit shows the author's avatar (Gravatar or generated initials) clipped to a circle, with a colored lane border ring</span></li>
  <li><span class="ind ind-head"></span><span><strong>Small filled dot</strong> — merge commit with two or more parents; rendered smaller than avatar nodes (~65% radius) to mark topology without visual clutter</span></li>
  <li><span class="ind ind-head"></span><span><strong>Outer glow ring</strong> — the current HEAD commit (checked-out)</span></li>
  <li><span class="ind ind-dimmed"></span><span><strong>Dimmed avatar</strong> — commit already pushed to the remote tracking ref</span></li>
  <li><span class="ind ind-wip"></span><span><strong>Dashed border</strong> — WIP node representing working directory changes</span></li>
  <li><span class="ind ind-amber"></span><span><strong>Amber dot (tab bar)</strong> — the repository has uncommitted changes</span></li>
</ul>

<h2>Author avatars</h2>
<p>
  For each visible commit, Arbor resolves the author's avatar using their commit email:
</p>
<ol class="step-list">
  <li>The email is hashed with <strong>SHA-256</strong> (via Web Crypto API — no external lib needed)</li>
  <li>A <strong>Gravatar</strong> lookup is attempted: <code>gravatar.com/avatar/&lt;sha256&gt;</code></li>
  <li>If no Gravatar exists, a deterministic <strong>colored circle with initials</strong> is generated client-side</li>
</ol>
<div class="callout info">
  <strong>GitHub &amp; GitLab</strong> — both platforms associate commit emails with Gravatar accounts by default, so avatars resolve automatically for most contributors. Users who have set a custom avatar only on GitHub/GitLab (not on Gravatar) will fall back to the generated initials avatar.
</div>
<p>Avatars are cached in memory for the session — each email is fetched at most once.</p>

<h2>Branch labels</h2>
<p>Labels appear inline on each commit row:</p>
<p>
  <span class="chip chip-local">feature/login</span>&ensp;local branch&ensp;·&ensp;
  <span class="chip chip-remote">origin/main</span>&ensp;remote tracking&ensp;·&ensp;
  <span class="chip chip-tag">v2.1.0</span>&ensp;tag&ensp;·&ensp;
  <span class="chip chip-head">HEAD</span>&ensp;checked-out commit
</p>

<h2>Graph rendering</h2>
<p>The lane layout is computed in Rust (<code>src-tauri/src/git/graph.rs</code>) using a gitk-style topological lane assignment. The frontend renders the result as an SVG with:</p>
<ul>
  <li><strong>Virtual scrolling</strong> — only the rows in the viewport (± 5 rows buffer) are rendered; <code>ROW_HEIGHT = 28px</code></li>
  <li><strong>Lane width</strong> — <code>LANE_WIDTH = 26px</code> per lane; <code>NODE_RADIUS = 10px</code> (20px avatar diameter)</li>
  <li><strong>Edges</strong> — right-angle elbows with rounded corners; dashed lines for squash-merge ghost edges</li>
  <li><strong>SVG <code>&lt;clipPath&gt;</code></strong> — one per visible non-merge node, keyed by commit OID, clips the avatar <code>&lt;image&gt;</code> to a circle</li>
  <li><strong>Pushed indicator</strong> — commits at or below the remote tracking ref row are dimmed (opacity 0.5) to distinguish pushed from unpushed</li>
</ul>

<h2>Context menu actions</h2>
<div class="feature-grid two-col">
  <div class="feature-card">
    <div class="fc-title">Branch &amp; Tag</div>
    <div class="fc-desc">
      <strong>Create Branch</strong> — branch from any commit<br>
      <strong>Create Tag</strong> — lightweight or annotated<br>
      <strong>Checkout</strong> — detached HEAD at that commit
    </div>
  </div>
  <div class="feature-card">
    <div class="fc-title">History Rewrite</div>
    <div class="fc-desc">
      <strong>Cherry-pick</strong> — apply commit to current branch<br>
      <strong>Revert</strong> — create a revert commit<br>
      <strong>Reset → here</strong> — soft / mixed / hard reset
    </div>
  </div>
  <div class="feature-card">
    <div class="fc-title">Remote</div>
    <div class="fc-desc">
      <strong>Push</strong> — push current branch (shown on HEAD only)<br>
      <strong>Pull</strong> — fetch + fast-forward current branch
    </div>
  </div>
  <div class="feature-card">
    <div class="fc-title">Clipboard</div>
    <div class="fc-desc">
      <strong>Copy SHA</strong> — full commit hash to clipboard<br>
      <strong>Copy message</strong> — commit summary text
    </div>
  </div>
</div>

<div class="callout warning">
  <strong>Cherry-picking or reverting a merge commit</strong>
  Merge commits have two parents, so both cherry-pick and revert are ambiguous on them: Git needs to know which side of the merge to keep. Arbor defaults to <strong>parent 1</strong> (the receiving branch) — equivalent to <code>git revert -m 1</code> / <code>git cherry-pick -m 1</code> — which targets the changes that were merged in while keeping the branch you merged onto as the baseline. This is what you want in almost every case; if you ever need the opposite, use the CLI with <code>-m 2</code>.
  <br><br>
  <em>Reset</em> is unaffected — it just moves <code>HEAD</code> to the target commit and never computes a diff, so the number of parents doesn't matter.
</div>

<h2>WIP node context menu</h2>
<p>The <strong>WIP node</strong> (dashed circle at the top of the graph) represents uncommitted working directory changes. <strong>Right-click</strong> it to access quick actions:</p>
<div class="feature-grid two-col">
  <div class="feature-card">
    <div class="fc-title">Open Stage Area</div>
    <div class="fc-desc">Loads the working directory diff and opens the Stage Area panel — the same as clicking the WIP node.</div>
  </div>
  <div class="feature-card">
    <div class="fc-title">Stash Changes</div>
    <div class="fc-desc">Saves all working directory changes (including untracked files) to the stash stack and restores a clean working tree.</div>
  </div>
  <div class="feature-card">
    <div class="fc-title">Stash (exclude untracked)</div>
    <div class="fc-desc">Same as above but leaves untracked files in place — only tracked modifications and staged changes are stashed.</div>
  </div>
</div>
<p>Stashes appear in the sidebar under <strong>Stashes</strong> and can be applied, popped, or dropped at any time.</p>

<h2>Pagination</h2>
<p>
  By default the graph loads history in batches of 500 commits (configurable in <em>Settings → Graph → Commits per load</em>).
  When you scroll near the bottom, the next batch is fetched automatically.
</p>
<p>
  You can disable this behaviour entirely in <em>Settings → Graph → Lazy-load pagination</em>.
  When pagination is off, the <strong>entire</strong> repository history is fetched in a single request on startup.
  This is convenient for small repos but can be slow for large ones.
  The setting is persisted to <code>~/.config/arbor/config.toml</code>.
</p>

<h2>File History Filter</h2>
<p>Filter the graph to show only commits that touched a specific file:</p>
<ol class="step-list">
  <li>In the diff file list, hover any file to reveal a <strong>file-search icon</strong> on the right</li>
  <li>Click it — the graph reloads with only the relevant commits visible</li>
  <li>A pill in the toolbar shows the active file name. Click <strong>×</strong> to clear the filter</li>
</ol>
<div class="callout info">
  <strong>Under the hood</strong>
  The filter runs in Rust via <code>DiffOptions::pathspec</code> — renames, copies, and binary files are all included. Pagination (load-more) also respects the active filter.
</div>

<h2>Commit Templates</h2>
<p>The commit message field is auto-filled from the first available source, in priority order:</p>
<ol class="step-list">
  <li><strong>Git native</strong> — the file pointed to by <code>commit.template</code> in your repo's <code>.gitconfig</code></li>
  <li><strong>Global Arbor template</strong> — set in <em>Settings → Repository → Commit Template</em>, applies to all repos</li>
</ol>
<p>A template icon appears in the top-right corner of the message field whenever the current text differs from the template — click it to restore without losing your changes.</p>

<h2>Export Graph as SVG</h2>
<p>The entire commit history can be exported as a standalone, fully-scalable SVG file — useful for documentation, pull-request overviews, or archiving a project's branching strategy.</p>

<h3>How to trigger</h3>
<ul>
  <li><strong>Toolbar</strong> — click the <span class="kbd-icon">↓</span> <em>Download</em> button at the top-right of the graph (visible when a graph is loaded)</li>
  <li><strong>Context menu</strong> — right-click any empty area of the graph background and choose <em>Export graph as SVG…</em></li>
</ul>
<p>A file-picker dialog opens so you can choose the output path and filename (default: <code>graph.svg</code>).</p>

<h3>What is exported</h3>
<ul>
  <li>The <strong>full history</strong> (up to 999 999 commits) — not just the currently visible page</li>
  <li><strong>Lane graph</strong> — same geometry as the on-screen render: <code>ROW_HEIGHT=28px</code>, <code>LANE_WIDTH=26px</code>, <code>NODE_RADIUS=10px</code>, bezier elbows</li>
  <li><strong>Colored lanes</strong> — the same 10-colour palette as the live graph</li>
  <li><strong>Node styles</strong> — merge commits get an outer ring; HEAD commit gets a white border ring</li>
  <li><strong>Ref badges</strong> — branch labels (local/remote) and tags appear inline next to each commit in colour-coded pill shapes</li>
  <li><strong>Text columns</strong> — short SHA · ref badges · author name · commit summary (truncated at 72 chars)</li>
</ul>

<h3>Background job</h3>
<p>
  The export runs as a <strong>background job</strong> so the UI stays responsive even for large repositories.
  Progress is visible in the <em>Jobs</em> overlay (click the status-bar spinner or the badge count).
  A <strong>bell notification</strong> appears when the export completes or fails.
</p>
<div class="callout info">
  The SVG is written as a streaming <code>BufWriter</code> directly to disk — the full file is never held in memory — so exports of repositories with tens of thousands of commits stay within normal memory usage.
</div>

<h2>Branch Cleanup</h2>
<p>The <strong>trash icon</strong> in the sidebar's <em>Local Branches</em> header opens the Branch Cleanup modal. It scans for all branches already merged into a target branch and lets you delete them in bulk — locally or on the remote. See the <strong>Branches</strong> section for full details.</p>
