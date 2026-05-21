<h1>Reflog</h1>

<p class="doc-lead">
  The <strong>Reflog</strong> panel shows a complete history of where <code>HEAD</code> has pointed —
  every checkout, commit, merge, rebase, and reset — even for commits no longer reachable from any branch.
</p>

<h2>Opening the panel</h2>
<p>Click the <strong>History</strong> icon (clock arrow) in the Activity Bar to toggle the Reflog sidebar.</p>

<h2>Reading an entry</h2>
<table class="shortcuts-table">
  <thead><tr><th>Element</th><th>Meaning</th></tr></thead>
  <tbody>
    <tr><td><code>HEAD@&#123;n&#125;</code> badge</td><td>Position in the reflog — <code>HEAD@&#123;0&#125;</code> is the most recent</td></tr>
    <tr><td>Hash chip (accent color)</td><td>7-character short OID of the commit HEAD moved <em>to</em></td></tr>
    <tr><td>Action badge</td><td>Type of operation that moved HEAD (see below)</td></tr>
    <tr><td>Message</td><td>Git's description of the operation, e.g. <em>checkout: moving from main to feature/x</em></td></tr>
    <tr><td>Relative time</td><td>When the operation occurred; hover for the full date/time</td></tr>
  </tbody>
</table>

<h2>Action types</h2>
<div class="feature-grid two-col">
  <div class="feature-card">
    <div class="fc-title">Commit</div>
    <div class="fc-desc">A new commit was created — ordinary <code>git commit</code>, amend, cherry-pick, or revert.</div>
  </div>
  <div class="feature-card">
    <div class="fc-title">Checkout</div>
    <div class="fc-desc">HEAD was moved to a different branch or detached to a specific commit.</div>
  </div>
  <div class="feature-card">
    <div class="fc-title">Merge</div>
    <div class="fc-desc">A merge was performed — fast-forward or three-way.</div>
  </div>
  <div class="feature-card">
    <div class="fc-title">Rebase</div>
    <div class="fc-desc">HEAD moved as part of a rebase operation (one entry per replayed commit).</div>
  </div>
</div>

<h2>Filters</h2>
<p>The toolbar exposes two filters:</p>
<ul>
  <li><strong>Type</strong> — filter by action kind (Commit, Checkout, Merge, Rebase, Other). Multiple types can be selected simultaneously.</li>
  <li><strong>Sort</strong> — switch between <em>Newest first</em> (default, matches git output) and <em>Oldest first</em>.</li>
</ul>
<p>The <strong>search box</strong> filters by message text or hash prefix in real time. Use the <strong>Clear</strong> chip to reset all active filters at once.</p>

<h2>Pagination</h2>
<p>
  Up to <strong>200 entries</strong> are loaded from the backend on open. The panel displays
  <strong>50 at a time</strong> — click <em>Show more</em> at the bottom to reveal the next 50.
  The count of remaining entries is shown inline.
</p>

<h2>Context menu actions</h2>
<p>Right-click any entry to access:</p>
<div class="feature-grid two-col">
  <div class="feature-card">
    <div class="fc-title">Checkout this commit</div>
    <div class="fc-desc">Detaches HEAD to the entry's commit OID. The graph refreshes automatically.</div>
  </div>
  <div class="feature-card">
    <div class="fc-title">Create branch here</div>
    <div class="fc-desc">Opens the <strong>New Branch</strong> modal pre-filled with the entry's hash — useful for recovering commits no longer reachable from any branch.</div>
  </div>
  <div class="feature-card">
    <div class="fc-title">Copy hash</div>
    <div class="fc-desc">Copies the full 40-character OID to the clipboard.</div>
  </div>
</div>

<div class="callout info">
  <strong>Recovering lost commits</strong> — if you accidentally reset a branch or dropped a stash,
  find the commit in the Reflog, right-click → <em>Create branch here</em> to restore it before
  Git's garbage collector runs (typically after 30–90 days).
</div>

<h2>Under the hood</h2>
<p>
  The backend reads the reflog via <code>git2::Repository::reflog("HEAD")</code> and returns a flat
  array of <code>ReflogEntry</code> structs:
</p>
<table class="shortcuts-table">
  <thead><tr><th>Field</th><th>Type</th><th>Description</th></tr></thead>
  <tbody>
    <tr><td><code>index</code></td><td><code>usize</code></td><td>Position in reflog (<code>HEAD@&#123;index&#125;</code>)</td></tr>
    <tr><td><code>id</code></td><td><code>String</code></td><td>Full OID HEAD moved <em>to</em></td></tr>
    <tr><td><code>id_old</code></td><td><code>String</code></td><td>Full OID HEAD moved <em>from</em></td></tr>
    <tr><td><code>message</code></td><td><code>String</code></td><td>Git's reflog message</td></tr>
    <tr><td><code>committer_name</code></td><td><code>String</code></td><td>Name from the reflog signature</td></tr>
    <tr><td><code>committer_time</code></td><td><code>i64</code></td><td>Unix timestamp of the operation</td></tr>
  </tbody>
</table>
