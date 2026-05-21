<h1>Tags &amp; Stash</h1>

<h2>Tags</h2>
<p>The <strong>Tags</strong> section in the sidebar lists all tags in the repo, sorted newest-first using semantic version ordering (<code>v1.2.3</code> style). Annotated tags show an <strong>A</strong> badge; tags created locally and not yet pushed show a purple <strong>local</strong> badge.</p>

<h3>Local-only badge</h3>
<p>Git itself has no notion of "tag not pushed yet" — once a tag is fetched with <code>--tags</code> it lands in <code>refs/tags/</code> indistinguishable from one created locally. Arbor tracks this distinction explicitly: tag names you create through Arbor are recorded in <code>.arbor/config.toml</code> under <code>local_only_tags</code> and removed when you push (or delete) them. The <strong>local</strong> badge in the sidebar reads from that list, so the state survives app restarts and is scoped per-repo.</p>

<h3>Nearest-tag indicator</h3>
<p>The status bar shows a <span class="chip chip-tag">v1.2.0</span> chip with the nearest ancestor tag from <code>HEAD</code> — equivalent to <code>git describe --tags --abbrev=0</code>. Click it to copy the tag name to the clipboard. Works intelligently across branch types:</p>
<ul>
  <li><strong>Integration branches</strong> (<code>main</code>, <code>develop</code>) — shows the last published version tag</li>
  <li><strong>Feature branches</strong> — shows the version tag the branch was cut from</li>
  <li><strong>Hotfix branches</strong> (e.g. <code>hotfix/1.2.x</code>) — shows the tag being patched (e.g. <code>v1.2.0</code>)</li>
</ul>

<h3>Interacting with tags</h3>
<ul>
  <li><strong>Click</strong> a tag in the sidebar → scrolls to the tagged commit in the graph.</li>
  <li><strong>Right-click</strong> a tag for a context menu. The available items adapt to whether the tag is still local-only or already on the remote:
    <ul>
      <li><strong>Copy value</strong> — copies the tag name to the clipboard.</li>
      <li><strong>Push to origin</strong> — only shown for tags with the <em>local</em> badge. Pushes <code>refs/tags/&lt;name&gt;</code> and clears the badge.</li>
      <li><strong>Elimina localmente</strong> — opens a confirmation modal, then removes the tag only from the local repo.</li>
      <li><strong>Elimina locale + origin</strong> — only shown when the tag exists on the remote. Opens a stronger confirmation modal warning that the action is irreversible, then pushes <code>:refs/tags/&lt;name&gt;</code> (empty source = delete on remote) and deletes the local ref.</li>
    </ul>
  </li>
</ul>

<h3>Creating tags</h3>
<p>Right-click any commit in the graph → <strong>Create Tag…</strong>. The modal's primary action is a <strong>split button</strong>:</p>
<ul>
  <li><strong>Create</strong> (left side) — creates the tag locally and flags it as <em>local</em> until pushed.</li>
  <li><strong>▾ chevron</strong> (right side) opens a small menu with <strong>Create &amp; Push</strong>, which creates the tag and pushes it to <code>origin</code> in one step.</li>
</ul>
<p>If you provide a message in the input, an annotated tag is created (<code>A</code> badge); otherwise it's a lightweight tag.</p>

<h2>Stash</h2>
<p class="doc-lead">The stash saves your working directory changes (and staged files) onto a stack so you can switch context without committing. The <strong>Stashes</strong> section in the sidebar lists all entries.</p>

<h3>Creating a stash</h3>
<p>There are three entry points:</p>
<div class="feature-grid">
  <div class="feature-card">
    <div class="fc-title">WIP node — context menu</div>
    <div class="fc-desc">
      Right-click the <strong>WIP node</strong> at the top of the graph.<br>
      <strong>Stash Changes</strong> — includes untracked files.<br>
      <strong>Stash (exclude untracked)</strong> — only tracked modifications and staged changes.
    </div>
  </div>
  <div class="feature-card">
    <div class="fc-title">Stage Area — stash button</div>
    <div class="fc-desc">The toolbar in the Stage Area has a stash icon. Clicking it opens a small form where you can type an <strong>optional message</strong> before stashing. Saves with <em>include untracked</em> enabled.</div>
  </div>
  <div class="feature-card">
    <div class="fc-title">Sidebar — Stash button</div>
    <div class="fc-desc">The <strong>RepoActions</strong> bar in the sidebar also has a stash shortcut with the same optional-message form.</div>
  </div>
</div>

<h3>Browsing stashes</h3>
<p>Each stash entry in the sidebar shows its message (or <code>stash@{'{N}'}</code> if no message was set). <strong>Click</strong> a stash to load its diff in the Detail panel — useful for reviewing what is saved before deciding whether to apply.</p>

<h3>Applying a stash</h3>
<div class="feature-grid two-col">
  <div class="feature-card">
    <div class="fc-title">Apply <span style="font-size:10px;opacity:0.6">(▶)</span></div>
    <div class="fc-desc">Re-applies the stash to the working directory. The stash entry is <strong>kept</strong> on the stack — useful when you want to apply the same changes to multiple branches.</div>
  </div>
  <div class="feature-card">
    <div class="fc-title">Pop <span style="font-size:10px;opacity:0.6">(↵)</span></div>
    <div class="fc-desc">Applies the stash and <strong>removes</strong> it from the stack in one step. Equivalent to <code>git stash pop</code>.</div>
  </div>
</div>
<p>Both actions are available as inline hover buttons on the stash row and in the right-click context menu.</p>

<h4>Apply outcomes</h4>
<p>The toast tells you exactly what happened — no need to <code>git status</code> after:</p>
<dl class="meta-grid">
  <dt><em>Stash applied</em></dt><dd>Default success — changes are now in the workdir.</dd>
  <dt><em>Stash popped &amp; dropped</em></dt><dd>Same but for <code>pop</code> — entry was removed from the stack.</dd>
  <dt><em>No changes — working tree already matches the stash</em></dt><dd>The workdir already contained every line of the stash. Apply is a no-op; pop additionally drops the entry (toast says <em>"Stash dropped"</em>). Distinct from generic success so you don't wonder where the diff went.</dd>
</dl>

<h3>Renaming a stash</h3>
<p>Click the <strong>pencil icon</strong> on a stash row (visible on hover) or use the right-click context menu → <strong>Rename</strong>. An inline text input replaces the message in-place:</p>
<table class="shortcuts-table">
  <thead><tr><th>Key</th><th>Action</th></tr></thead>
  <tbody>
    <tr><td><kbd>Enter</kbd></td><td>Confirm rename</td></tr>
    <tr><td><kbd>Escape</kbd></td><td>Cancel without saving</td></tr>
  </tbody>
</table>

<h3>Dropping a stash</h3>
<p>Click the <strong>trash icon</strong> (red on hover) or right-click → <strong>Drop</strong>. The entry is permanently removed from the stack.</p>

<h3>Conflict handling</h3>
<p>Three situations can interrupt an apply or pop — all three now flow through the <strong>same modal</strong> so you don't bounce between dialogs:</p>
<div class="feature-grid">
  <div class="feature-card">
    <div class="fc-title">Local-changes-overwritten</div>
    <div class="fc-desc">Tracked files in the workdir would be replaced. Per-row choice: keep your version, take the stash version, or skip.</div>
  </div>
  <div class="feature-card">
    <div class="fc-title">Blocking untracked files</div>
    <div class="fc-desc">Untracked workdir files would be overwritten by stashed untracked content. Same per-row controls.</div>
  </div>
  <div class="feature-card">
    <div class="fc-title">Merge conflicts</div>
    <div class="fc-desc">Tracked files with overlapping edits — guided two-column + result resolver, identical to the merge flow.</div>
  </div>
</div>
<p>Bytes that already match between workdir and stash are filtered out before the modal opens, so identical files don't show up as blockers (silent-apply path).</p>
<div class="callout info">
  <strong>Pull auto-stash</strong> — when you pull a branch with a dirty working directory, Arbor automatically stashes first, pulls, then pops the stash. If the pop has conflicts the same resolution modal appears with the original stash entry preserved.
</div>
