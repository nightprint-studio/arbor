<!-- Bennu docs — local history: the record of a file's own past, independent of git. -->
<h1>Local history</h1>
<p class="doc-lead">
  Every version of a file Bennu has seen, whether or not it was ever committed — including the
  files that are no longer there.
</p>

<h2>Local history</h2>
<p>
  Bennu keeps its own record of what every project file used to be — independent of git, and
  underneath it. Git protects you from the commit onwards; most of the ways a file is lost happen
  before that: a save over something you wanted, a refactor that went wider than you thought, a
  delete of a file that was never committed, a tool that rewrote the buffer while you were looking
  elsewhere.
</p>
<p>
  Open it with <kbd>Alt</kbd> + <kbd>Shift</kbd> + <kbd>H</kbd>, from <strong>Local History</strong>
  in the editor's or the project tree's right-click menu, or from the Command Palette. The dialog has
  four scopes: <strong>this file</strong>, the <strong>folder</strong>, the whole
  <strong>project</strong>, and <strong>deleted</strong> — the files the project no longer has. That
  last one is a scope of its own for a reason: a deleted file has no row anywhere to right-click, so a
  list that does not depend on the filesystem is the only route back to it. Deleted files also stay
  visible in the folder scope, struck through and where you left them, with a Restore beside them.
</p>
<p>
  A revision is recorded when you save, when a tool writes (a refactor, a generate, a format), when a
  file changes or vanishes <em>outside</em> Bennu, and when you rename or create one. Not on every
  keystroke — the editor's own undo covers that scale. Each row says which of those it was, because
  a save is you and a refactor may have touched a dozen files at once; a refactor's row shows every
  file it touched, marked in the folder column.
</p>
<p>
  The right-hand pane is the comparison: the revision you picked against what is on disk now. It
  opens <strong>side by side</strong> — the old text left, the current right, each changed line
  opposite the line that replaced it, and a hatched band where one side has nothing. The button in
  that pane's header (or <kbd>Alt</kbd> + <kbd>D</kbd>) switches to the <strong>unified</strong>
  patch, one column with <code>+</code> and <code>−</code>, for when you want to read the change as
  a change. Whichever you leave it on is the one it opens with next time.
</p>
<p>
  <strong>Put Label…</strong> pins a name on a moment (<em>before the atlas refactor</em>), and a
  labelled revision never expires. Everything else is kept for the number of days set in
  <strong>Settings → Editor → Local history</strong>, within a size budget per project; each file's
  newest revision always survives both. Files git ignores are not recorded — build output is
  regenerated, not recovered. Nothing is written inside the project: the store lives in Arbor's data
  folder, so it is never something you could commit by accident.
</p>
