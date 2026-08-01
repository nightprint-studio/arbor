<script lang="ts">
  /**
   * Garrulus docs — syncing.
   *
   * The page this product is for. It states the guarantee first (nothing writes
   * without a click) because every other behaviour on the page is a consequence
   * of it, and because it is the thing a reader needs in order to trust the
   * button.
   */
</script>

<h1>Syncing</h1>
<p class="doc-lead">
  A vault can be mirrored to a destination, so the same notes are on both machines.
  There is exactly one control for it — the button in the title bar — and one rule that
  governs all of it: <b>nothing writes without a click</b>.
</p>

<h2>The rule</h2>
<p>
  Garrulus never commits, pushes, pulls or edits a file on its own. Every byte that
  moves, in either direction, moved because you pressed something. There is no timer
  that sends your notes, no save that quietly publishes, and no background job that
  merges anything.
</p>
<p>
  The background does exactly one thing, and it cannot change a file: it <b>asks the
  destination whether anything is new</b> and updates the button. That check runs while
  the window has focus, and stops when it does not. It is what makes the button's
  answer true — "there is nothing new from the other machine" is only worth reading if
  something actually looked.
</p>
<p>
  The consequence is a history you can read. A vault that committed itself every twenty
  seconds would have a log of a thousand entries saying nothing; a vault that commits
  when you say so has a log of the times you finished something.
</p>

<h2>The button, and what each state means</h2>
<p>
  It is a split button: the left half does the obvious thing for the state it is in, the
  caret opens the rest. The colour is the state, never decoration, and the count rides
  in a badge so the button is readable without hovering it.
</p>
<table>
  <thead>
    <tr><th>It says</th><th>Which means</th><th>Pressing it</th></tr>
  </thead>
  <tbody>
    <tr>
      <td><b>Aligned</b>, with where and when</td>
      <td>Everything here is there, and everything there is here. The subtitle names the
        machine that last wrote, and how long ago.</td>
      <td>Checks again, now.</td>
    </tr>
    <tr>
      <td><b>n notes to send</b></td>
      <td>You have written things this destination does not have yet — edited, or
        finished and not yet sent. The two are one number because the difference is not
        one you asked to be told about.</td>
      <td>Sync: commit what changed, pull, push.</td>
    </tr>
    <tr>
      <td><b>n notes coming in</b></td>
      <td>The other machine wrote things this one does not have. Nothing of yours is
        waiting.</td>
      <td>Pull.</td>
    </tr>
    <tr>
      <td><b>n to send · n coming in</b></td>
      <td>Both sides moved since they last agreed. Normal, not a problem.</td>
      <td>Sync — what comes in is integrated under what you wrote, so the history stays
        a single line.</td>
    </tr>
    <tr>
      <td><b>n conflicts to resolve</b></td>
      <td>Something arrived that could not be merged without choosing. It outranks every
        other state, because it is the only one that can cost you text if ignored.</td>
      <td>Opens Conflicts in the bottom dock.</td>
    </tr>
    <tr>
      <td><b>Syncing…</b></td>
      <td>A sync you asked for is running.</td>
      <td>Cancels it.</td>
    </tr>
    <tr>
      <td><b>Not reachable</b></td>
      <td>A destination is configured but did not answer. Counts are hidden rather than
        shown stale — a number nothing refreshed is worse than no number.</td>
      <td>Tries again.</td>
    </tr>
    <tr>
      <td><b>No destination</b></td>
      <td>This vault is local to this machine. A perfectly good way to use it.</td>
      <td>Opens the destination settings.</td>
    </tr>
  </tbody>
</table>
<p>
  The caret carries the rest: pull only, push only, commit with a message of your own,
  show what would be sent, conflicts, this note's history, and where the vault syncs to.
  Each of them is also a command palette entry, and <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>S</kbd>
  is the whole sync in one keystroke.
</p>

<h2>What a sync does, in order</h2>
<ol>
  <li>Everything you have edited is committed together, described for you — <i>New note:
    Bug — crash on startup</i>, <i>3 notes updated</i> — and attributed to this machine
    by name, so the history reads as a log of where you were working. <i>Commit with a
    message…</i> is there for the change that deserves one.</li>
  <li>What the other machine wrote is brought in and placed <i>under</i> what you wrote,
    so the history stays a single readable line rather than a braid.</li>
  <li>The result is sent.</li>
</ol>
<p>
  If step 2 produces a conflict, step 3 does not run. Sending a half-resolved vault is
  how the other machine inherits your problem.
</p>
<p>
  <b>Notes open while a sync brings something in</b> update in place when you had not
  touched them. When you had, nothing is overwritten: a banner appears on that note
  offering the incoming version, yours, or a diff of the two.
</p>

<h2>Conflicts</h2>
<p>
  Two machines and one vault means conflicts are ordinary, not exceptional. The
  guarantee is narrow and absolute: <b>text you typed is never dropped, overwritten or
  silently merged</b>. Anything ambiguous becomes something visible you decide about.
</p>
<p>Most of what looks like a conflict never becomes one:</p>
<ul>
  <li><b>Fields merge field by field.</b> Frontmatter is structured, so changing the
    status here and the severity there is not a disagreement. Only the same field
    changed on both sides is.</li>
  <li><b>Bodies merge line by line</b> against the version both sides started from. Two
    people editing one note usually edit different parts of it, and those merge
    cleanly.</li>
  <li><b>The daily note is appended, not merged.</b> Two machines adding lines to the
    same day is the most common collision there is, and a three-way merge is the wrong
    answer to it — the right one is both days' entries, in time order.</li>
  <li><b>Garrulus's own settings never conflict.</b> Types are unioned, settings take
    the most recent value per key. A conflict in a settings file is pure noise.</li>
</ul>

<h3>What is left, and where it goes</h3>
<p>
  When a genuine disagreement survives all of that, <b>your note is left exactly as you
  wrote it</b>. The incoming version is written beside it as a second file, named for
  where it came from and when — <code>Note (conflict — home, 31-07 14:22).md</code> —
  and an entry appears under <b>Conflicts</b> in the bottom dock.
</p>
<p>
  <b>Your file never contains merge markers.</b> No <code>&lt;&lt;&lt;&lt;&lt;&lt;&lt;</code>,
  no half-merged paragraph. The vault stays readable, and openable in any other editor,
  in the middle of an unresolved conflict.
</p>
<p>The Conflicts panel shows the two versions side by side, and offers three answers:</p>
<ul>
  <li><b>Keep mine</b> — the note stays as it is, the second file is removed.</li>
  <li><b>Take theirs</b> — the incoming version becomes the note.</li>
  <li><b>Merge by hand</b> — edit the note with both versions in front of you, then
    resolve it as yours. This is the honest option for the case where both sides were
    right.</li>
</ul>
<p>
  Nothing is decided for you and nothing expires. A conflict left open is a conflict
  still open tomorrow, on both machines.
</p>

<h2>Where a vault can sync to</h2>
<ul>
  <li><b>A git repository.</b> The full behaviour: every note keeps a version history,
    any past version can be read, diffed and restored, and conflicts are detected
    exactly. That the vault is a git repository is otherwise invisible — Garrulus shows
    no branch, no commit and no staging area unless you go looking for them.</li>
  <li><b>A folder.</b> A USB stick, a network share, or a directory something else
    already synchronises for you. There is no history behind a folder, so the history
    panel is hidden rather than shown empty, and concurrent edits are caught by
    comparing timestamps and contents. A real collision produces the same conflict, with
    the same guarantee.</li>
</ul>

<h3>Private repositories</h3>
<p>
  A repository created from Garrulus is <b>private</b>. There is no public option
  anywhere in the interface, at any layer: a personal note vault has no business being
  public, and it is not a mistake that can be taken back — by the time you notice, the
  content has been read. A vault you genuinely want public can be made public
  deliberately, on the provider's own site.
</p>
<p>
  Signing in is Arbor's, not this window's: the credentials you already use for your git
  provider are what this uses, and no token is ever stored by Garrulus itself.
</p>

<h2>When the destination is not reachable</h2>
<p>
  Offline is the normal case, not a failure. The editor never waits for the network:
  writing, linking, searching and creating notes all work exactly the same with nothing
  connected, and what you wrote is sent when you ask, later.
</p>
<p>
  Retries are quiet. A single missed check is a dropped packet and is not worth a
  message, so nothing is said until a retry has also failed — and then it is said once,
  not once per attempt. If it comes back, you are told once, and only if you were told
  it had gone. After enough failed attempts the retrying stops and says so, rather than
  spinning in the background pretending.
</p>

<h2>History of a note</h2>
<p>
  On a git destination every note carries its own history: who wrote each version — for
  Garrulus's own commits, <i>which machine</i> — when, and what changed. Any version can
  be compared with the current one and restored. A restore is an ordinary edit: it
  writes the old text into the note, and <kbd>Ctrl</kbd>+<kbd>Z</kbd> takes it back.
</p>

<h2>Attachments</h2>
<p>
  Images and files pasted into a note are stored in the vault's attachments folder and
  travel with it. They are also the one thing that can make a vault heavy: a repository
  keeps every version of every binary forever. Garrulus warns above a size threshold and
  can list what nothing references, but the rule of thumb is worth stating plainly — a
  note vault is not where a 200 MB video belongs.
</p>
