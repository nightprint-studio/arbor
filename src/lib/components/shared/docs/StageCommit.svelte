<h1>Stage &amp; Commit</h1>

<p class="doc-lead">The stage area is your workspace for crafting commits. Stage individual files, specific hunks, or even single lines — then write your message and commit.</p>

<div class="callout tip">
  <strong>Open Stage Area</strong>
  Press <kbd>Ctrl+Shift+S</kbd> or click the commit icon at the bottom of the Activity Bar.
</div>

<h2>Basic workflow</h2>
<ol class="step-list">
  <li>Edit files in your editor — they appear in the <strong>Unstaged</strong> list automatically</li>
  <li>Click a file to preview its diff in the detail panel</li>
  <li>Click <strong>+</strong> next to a file (or <strong>Stage All</strong>) to queue it for commit</li>
  <li>Write your message in the text area — the first line becomes the commit summary</li>
  <li>Press <kbd>Ctrl+Enter</kbd> to commit</li>
</ol>

<h2>File list sections</h2>
<div class="feature-grid two-col">
  <div class="feature-card">
    <div class="fc-title">Unstaged</div>
    <div class="fc-desc">Modified, new, or deleted files not yet queued for commit. Click to preview. Right-click for quick actions: Stage, Discard, Open in editor.</div>
  </div>
  <div class="feature-card">
    <div class="fc-title">Staged</div>
    <div class="fc-desc">Files queued for the next commit. Click to preview the staged diff. Right-click to unstage or discard changes.</div>
  </div>
</div>

<p>File status icons:&ensp;<strong>M</strong> modified&ensp;·&ensp;<strong>A</strong> added&ensp;·&ensp;<strong>D</strong> deleted&ensp;·&ensp;<strong>R</strong> renamed&ensp;·&ensp;<strong>U</strong> untracked</p>

<h2>Partial staging — hunk &amp; line level</h2>
<p>You don't have to stage whole files. In the diff viewer:</p>
<ul>
  <li><strong>Stage hunk</strong> — click the <strong>+</strong> button in a hunk header to stage that block of changes only</li>
  <li><strong>Stage individual lines</strong> — click line checkboxes to select specific additions or deletions, then click <strong>Stage Lines</strong></li>
  <li><strong>Unstage lines</strong> — the same mechanism works in reverse on the staged diff</li>
</ul>
<div class="callout info">
  <strong>How it works</strong>
  Arbor builds a custom unified diff patch from your selection and applies it to the index — the same technique as <code>git add -p</code>, but visual.
</div>

<h2>Diff navigation</h2>
<p>Every diff view (Stage, commit detail, branch compare, conflict modal) carries the same chunk-aware controls in its header:</p>
<ul>
  <li><strong>Chunk navigation</strong> — the <kbd>↑</kbd> / <kbd>↓</kbd> chevrons in the header (with a <em>n/N</em> counter) jump between change blocks. <kbd>F3</kbd> / <kbd>Shift+F3</kbd> do the same from the keyboard.</li>
  <li><strong>Auto-focus on open</strong> — opening a file (or staging a line) lands you on the first remaining change instead of the top of the file.</li>
  <li><strong>Show full file</strong> — the <em>file</em> icon next to the Unified/Split toggle expands the diff to the entire file with change highlights instead of just the N-line context. Useful for navigating a change in the surrounding code; toggleable per session, persisted in <em>config.toml</em>.</li>
  <li><strong>Virtualization</strong> — large diffs (default: more than 200 lines) automatically switch to a windowed renderer that only paints visible rows, so scrolling stays smooth even on giant files. Threshold is configurable in <em>Settings → Diff &amp; Stage</em>.</li>
</ul>

<h2>Commit form</h2>
<p>The commit requires at least one staged file and a non-empty message. The first line becomes the summary; anything after a blank line becomes the extended body — standard git convention.</p>
<p>Press <kbd>Ctrl+Enter</kbd> in the message field to commit immediately.</p>

<h2>Discarding changes</h2>
<p>Discarding restores a file (or all files) to match the current index. <strong>This is irreversible</strong> — any working-directory edits are permanently lost.</p>
<ul>
  <li><strong>Discard file</strong> — click the <kbd>↩</kbd> button next to a file, or right-click → <em>Discard Changes</em>. If the confirmation dialog is enabled, a modal will appear listing the file name before proceeding.</li>
  <li><strong>Discard All</strong> — click the <kbd>↩</kbd> (rotate-CCW) icon in the <em>Unstaged</em> section header. Always shows a confirmation modal that states how many files will be affected.</li>
</ul>
<div class="callout warning">
  <strong>Confirmation dialog</strong>
  By default Arbor asks you to confirm every single-file discard. You can turn this off in <em>Settings → Diff &amp; Stage → Confirm before discarding</em>. The Discard All modal is always shown regardless of this setting.
</div>

<h2>Stash</h2>
<p>Click the <strong>archive icon</strong> in the unstaged header (or in the sidebar actions bar) to stash all working directory changes. Optionally add a description. Stashes appear in the sidebar under <strong>Stashes</strong> and can be applied, popped, or dropped at any time.</p>

<h2>Merge conflicts</h2>
<p>During a merge or rebase, conflicted files appear in a dedicated <strong>Conflicts</strong> section instead of the normal unstaged list. Click <strong>Resolve Conflicts</strong> to open the guided three-panel resolution modal — see <em>Merge Conflicts</em> for the full workflow.</p>
