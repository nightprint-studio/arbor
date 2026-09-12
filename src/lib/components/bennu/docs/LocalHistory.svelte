<script lang="ts">
  /**
   * Local history: Bennu's own record of every version of a file, underneath git — how to open it, its four scopes, when a
   * revision is taken, comparing, labels and retention.
   */
  import Callout from '$lib/components/shared/ui/Callout.svelte';
</script>

<span class="eyebrow">Editor</span>
<h1>Local history</h1>

<p class="doc-lead">
  Every version of a file Bennu has seen, whether or not it was ever committed — including the files that are no longer there.
</p>

<h2>Why it exists</h2>
<p>
  Bennu keeps its own record of what every project file used to be, independent of git and underneath it. Git protects you from the
  commit onwards; most of the ways a file is lost happen before that:
</p>
<ul>
  <li>a save over something you wanted;</li>
  <li>a refactor that went wider than you thought;</li>
  <li>a delete of a file that was never committed;</li>
  <li>a tool that rewrote the buffer while you were looking elsewhere.</li>
</ul>

<h2>Opening it</h2>
<p>
  <kbd>Alt</kbd> + <kbd>Shift</kbd> + <kbd>H</kbd>, <strong>Local History</strong> in the editor's or the project tree's right-click menu,
  or the command palette. The dialog has four scopes:
</p>
<div class="feature-grid two-col">
  <div class="feature-card">
    <div class="fc-title">This file</div>
    <div class="fc-desc">Every revision of the file in front of you.</div>
  </div>
  <div class="feature-card">
    <div class="fc-title">The folder</div>
    <div class="fc-desc">Deleted files stay visible here, struck through where you left them, with a Restore beside them.</div>
  </div>
  <div class="feature-card">
    <div class="fc-title">The project</div>
    <div class="fc-desc">Everything, across every file.</div>
  </div>
  <div class="feature-card">
    <div class="fc-title">Deleted</div>
    <div class="fc-desc">The files the project no longer has. A deleted file has no row to right-click, so a list that does not depend on the filesystem is the only way back to it.</div>
  </div>
</div>

<h2>When a revision is recorded</h2>
<table>
  <thead><tr><th>When</th><th>The row says</th></tr></thead>
  <tbody>
    <tr><td>You save</td><td>A save — you</td></tr>
    <tr><td>A tool writes — a refactor, a generate, a format</td><td>Which tool; a refactor's row shows every file it touched, marked in the folder column</td></tr>
    <tr><td>A file changes or vanishes <em>outside</em> Bennu</td><td>An outside change</td></tr>
    <tr><td>You rename or create a file</td><td>The rename or the creation</td></tr>
  </tbody>
</table>
<p>
  Not on every keystroke: the editor's own undo covers that scale.
</p>

<h2>Comparing</h2>
<p>
  The right-hand pane compares the revision you picked with what is on disk now. It opens <strong>side by side</strong> — the old text on
  the left, the current on the right, each changed line opposite the line that replaced it, and a hatched band where one side has nothing.
</p>
<p>
  The button in that pane's header, or <kbd>Alt</kbd> + <kbd>D</kbd>, switches to the <strong>unified</strong> patch — one column with
  <code>+</code> and <code>−</code> — for reading the change as a change. Whichever you leave it on is the one it opens with next time.
</p>

<h2>Labels and how long it keeps</h2>
<p>
  <strong>Put Label…</strong> pins a name on a moment — <em>before the atlas refactor</em> — and a labelled revision never expires.
  Everything else is kept for the number of days set in <strong>Settings → Editor → Local history</strong>, within a size budget per
  project, and each file's newest revision always survives both.
</p>
<Callout variant="info" title="What is not recorded, and where it lives">
  Files git ignores are not recorded: build output is regenerated, not recovered. And nothing is written inside the project — the store
  lives in Arbor's data folder, so it is never something you could commit by accident.
</Callout>
