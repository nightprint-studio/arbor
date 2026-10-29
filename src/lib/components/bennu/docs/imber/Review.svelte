<span class="eyebrow">Imber</span>
<h1>Imber: Reviewing changes</h1>

<p class="doc-lead">
  Whatever Imber changes can be read first, written all-or-nothing, undone afterwards, and recovered from Local History; risky work can happen in an isolated copy that touches nothing until you apply it.
</p>

<h2>Proposals</h2>
<p>
  In Manual mode a change is a <strong>proposal</strong>: a card in the thread listing the files with their added and removed lines. Nothing is written yet. A change made by one tool call (an edit across files, a rename, a replace over a folder) is one proposal, written all or nothing.
</p>
<p>
  <strong>Review</strong> opens the proposal as a diff, unified or side by side, with the files in a list on the left (<kbd>↑</kbd> / <kbd>↓</kbd> move between them). Decide with <strong>Apply all</strong> (<kbd>Ctrl</kbd> + <kbd>Enter</kbd>) or <strong>Reject all</strong>. A rejected card turns red and keeps its files readable; an applied one stays as a record and can be undone. A proposal that touches a file of another project names it as <code>project:path</code>.
</p>

<h2>Applied changes and Undo</h2>
<p>
  In Accept edits and Autonomous, edits inside the reach are applied as they are proposed. The card stays as a record and has <strong>Undo</strong>: it puts back exactly the files that proposal wrote, all of them or none, and refuses if one of the files changed since. Every write is also recorded in Local History.
</p>

<h2>Outdated proposals</h2>
<p>
  A proposal is applied as a whole. Bennu first checks that every file is still as it was when Imber read it. If any changed (you typed, or another proposal went in first) <em>nothing</em> is written and the card turns <strong>Outdated</strong>, marking the files that changed. It stays in the conversation with three ways forward:
</p>
<ul>
  <li><strong>Re-apply on current file</strong> (<kbd>Ctrl</kbd> + <kbd>Enter</kbd> in the review window): makes each change again on the file as it is now, when its lines are still there exactly once, and gives you the result to approve.</li>
  <li><strong>Ask to redo</strong>: drops it and asks Imber, in the conversation, to propose it again.</li>
  <li><strong>Discard</strong>: drops it.</li>
</ul>
<p>
  If a write fails after others went in, those files are put back, so a proposal is never half applied. A file that another undecided proposal also edits is flagged on the card: applying either makes the other outdated.
</p>

<h2>Local History</h2>
<p>
  Everything Imber writes in the project is recorded in Bennu's Local History, deletions included (also the ones applied from an isolated copy). Imber can list a file's revisions, show what changed, list deleted files and restore one; a restore is an edit like any other, proposed or applied by your mode. You can do the same yourself from the editor; see Local history.
</p>

<h2>Isolated copy</h2>
<p>
  The composer's settings menu has <em>Isolated copy</em>, off by default and fixed when a conversation starts. Edits land in the copy at once and nothing reaches your real files until you apply.
</p>
<table>
  <thead><tr><th>Kind</th><th>What it is</th><th>What works</th></tr></thead>
  <tbody>
    <tr><td><strong>Copy — text only</strong></td><td>In a git repository (with git access on) a detached git worktree, so nothing is copied; otherwise a plain copy of the sources, capped at 512 MB or 40,000 files, leaving out version control, dependency folders and build output.</td><td>Files, search, build, run and live tools. No code intelligence (usages, symbols, problems, rename, beans) and no indexing, so it is cheap.</td></tr>
    <tr><td><strong>Git worktree — indexed</strong></td><td>A real worktree opened and indexed as a hidden project. Offered only for a git repository with git access on; never appears in your projects; shows its indexing progress in the conversation.</td><td>Every tool.</td></tr>
  </tbody>
</table>
<ul>
  <li>A worktree starts from what you see: files you had changed, staged or not yet added are copied over it.</li>
  <li>Both live in the app data folder, never beside the repository. While one is active a banner says so.</li>
  <li><strong>Apply to project</strong> (<kbd>Ctrl</kbd> + <kbd>Shift</kbd> + <kbd>Enter</kbd> with focus in the panel, or <em>Imber: Review and apply the isolated copy</em>) shows the combined diff first. Applying is all or nothing: if you, another conversation or a tool changed a file in the project that the copy also changed, nothing is written and each such file is listed with the reason. Deletions and renames are included and the written files are recorded in Local History.</li>
  <li><strong>Discard</strong> (<em>Imber: Discard the isolated copy…</em>) removes the copy, its index and the processes started in it. Deleting the conversation does too. A copy left behind by a crash, or unused for 30 days beyond the 12 most recent, is swept at startup.</li>
  <li>Services started inside a copy are not shown in the Live panel of the project. A conversation in a copy has its own tree and never queues against the project's builds.</li>
  <li>You cannot apply or discard while Imber is still working in the copy: stop it first.</li>
</ul>
<p>
  Start one from the composer's menu or the palette: <em>Imber: New conversation in an isolated copy (text only)</em>, <em>Imber: New conversation in a git worktree (indexed)</em>.
</p>
