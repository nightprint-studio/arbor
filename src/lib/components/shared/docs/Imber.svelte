<script lang="ts">
  import Kbd from '$lib/components/shared/internal/Kbd.svelte';
</script>

<h1>Imber (assistant)</h1>
<p><strong>Imber</strong> is the assistant of the whole suite — the same one that works in Bennu. In Corvus it works on the repository of the active tab: it commits, pushes, fetches, pulls, creates branches, merges, opens merge requests, and explains the history. It uses the same git code as the buttons, so a commit made by Imber runs the same hooks, recovery snapshots and credential flow as one made by hand.</p>

<h2>The same panel as Bennu's</h2>
<p>Corvus and Bennu draw the assistant with the same widgets: the home screen with its starter cards, the conversation with its bubble, the folded tool receipts and the activity line, and the composer with its mode menu, send and stop buttons. What differs is what goes in them — Corvus offers the git starters, the Operate / Chat switch, Undo and Schedules, and keeps one conversation per repository; Bennu adds attachments, skills, the model picker and the conversation list.</p>

<h2>One setup for both products</h2>
<p>The API key, the model, the effort, the spending limits and the price list are chosen once and shared with Bennu. The key sits in the system credential vault (never in a file); store it from the panel the first time, or from Bennu's settings. Corvus has one setting of its own: the mode.</p>

<h2>Open it</h2>
<p><Kbd action="imber_toggle" size="sm" /> or <strong>Imber: Open Assistant</strong> in the Command Palette opens the panel on the right; the sparkle at the top of the right activity bar does the same (hide or move it from <em>Customize Activity Bar</em>). Without a repository open it asks you to open one; without an API key the panel asks for it. Each repository has its own conversation; switching tab switches conversation.</p>

<h2>Operate and Chat</h2>
<ul>
  <li><strong>Operate</strong> has the git tools: status, diff, log (by file, author, words, dates, or a range like <code>main..feature/x</code>), show, blame, branches, merge requests, stage, unstage, commit, fetch, pull, push, create branch, switch branch, merge, open a merge request, delete branch, build, edit, run, follow, stop and retry <strong>plans</strong>, and undo its own local changes.</li>
  <li><strong>Chat</strong> has no tools. It cannot see or change the repository; it answers questions about git and about how to do things in Corvus.</li>
</ul>
<p>Requests can chain steps — "create a branch, commit what is staged, then merge it into dev". Imber does them one at a time and stops at the first failure.</p>

<h2>Manual and Auto</h2>
<table class="shortcuts-table">
  <thead><tr><th>Mode</th><th>What happens to a change</th></tr></thead>
  <tbody>
    <tr><td><strong>Manual</strong> (default)</td><td>Every change — stage, commit, fetch, pull, branch, merge, push, merge request — is shown as a card with the exact git command and what it does. Nothing runs until you approve.</td></tr>
    <tr><td><strong>Auto</strong></td><td>Changes run at once. Reading (status, diff, log, blame) never asks in either mode.</td></tr>
  </tbody>
</table>
<p><strong>Force push and deleting a branch always ask, in both modes.</strong> Imber cannot rebase, reset, rewrite history or resolve conflicts; for those it tells you to use Corvus. A merge that conflicts is left in progress, as if you had run it yourself.</p>
<p>While a card waits: <Kbd keys={['Ctrl', 'Enter']} size="sm" /> approves, <Kbd keys={['Esc']} size="sm" /> declines. <Kbd keys={['Esc']} size="sm" /> while Imber works stops the run. Switch the mode with the toggle in the panel or <strong>Imber: Switch to Auto Mode</strong> in the palette. The mode is remembered.</p>

<h2>Commit message from the staged changes</h2>
<p>The sparkle in the corner of the commit box, <Kbd action="imber_commit_message" size="sm" /> in the box, or <strong>Imber: Generate Commit Message</strong> writes a message from what is staged. It reads the style of the repository's recent commits (Conventional Commits, a ticket prefix, or plain sentences), follows <code>commit.template</code> when there is one, and uses the ticket in the branch name. If the box already holds text, that text is treated as a hint and improved. The message is only put in the box: you edit it, and commit as usual. <Kbd keys={['Esc']} size="sm" /> stops it. It uses the cheaper side-job model chosen in Imber's settings.</p>

<h2>Schedules</h2>
<p>Imber can do things at a time you choose: once, every N minutes, every day, on chosen weekdays, or on a cron expression (6 fields: sec min hour day month weekday; times are local). <strong>Imber: Schedules…</strong> in the palette, or the clock in the panel header, opens the window that lists <em>every</em> schedule — what it does, when, whether it is on, its next run and how the last one ended — and creates, edits, turns on or off, runs now, and deletes them. You can also ask Imber: "every weekday at 9, check what changed on main" (with approval in Manual mode).</p>
<table class="shortcuts-table">
  <thead><tr><th>Action</th><th>What it does at the time</th></tr></thead>
  <tbody>
    <tr><td><strong>Wake this conversation</strong> (<em>awake</em>)</td><td>Sends the saved prompt into the same conversation, which keeps its history, so Imber picks up where it stopped. If the conversation no longer exists (Corvus was restarted) a new one starts.</td></tr>
    <tr><td><strong>Ask Imber in a new conversation</strong></td><td>A fresh conversation each time; the outcome is shown on the schedule.</td></tr>
    <tr><td><strong>Fetch</strong></td><td>A plain <code>git fetch</code>: no model, no cost.</td></tr>
  </tbody>
</table>
<p>In the window: <Kbd keys={['↑', '↓']} size="sm" /> move, <Kbd keys={['Enter']} size="sm" /> edits, <Kbd keys={['Space']} size="sm" /> turns on or off, <Kbd keys={['R']} size="sm" /> runs now, <Kbd keys={['Del']} size="sm" /> deletes, <Kbd keys={['N']} size="sm" /> creates; <Kbd keys={['Ctrl', 'Enter']} size="sm" /> saves in the editor.</p>
<ul>
  <li><strong>Nobody is there to approve.</strong> A scheduled run is <em>read-only</em> unless the schedule has <em>Let it make changes</em> on, in which case stage, commit, pull and the like run without asking. Force pushes and deletions never run unattended, and a scheduled run cannot create or change schedules. A schedule Imber creates can be no freer than the conversation that created it.</li>
  <li><strong>Only while Corvus is open.</strong> A run whose time passed while it was closed is <em>skipped</em> (the default) or <em>run once</em> on the next start, per schedule. A one-time schedule that was missed and skipped says so and turns itself off.</li>
  <li>Schedules are saved in the Corvus config folder and survive restarts. AI clients over MCP can list schedules and create read-only ones.</li>
</ul>

<h2>Plans</h2>
<p>For chains that must wait on something remote — "open an MR, and when it is merged start one on the other repository" — Imber builds a <a href="#plans">plan</a>: phases, gates (an MR merged, a delay, your go-ahead) and a <em>finally</em>, with push, create / merge MR, pipeline, tag and pull steps. It can add and remove steps, set gates, <strong>run</strong>, follow, <strong>stop</strong>, <strong>retry</strong> or <strong>skip</strong> a step, and open a gate. A commit cannot be planned (it captures the working tree now): it is done first.</p>
<ul>
  <li>The plan appears in the chat as a <strong>card</strong>: every phase and step with its state and outcome (the MR it opened, the error it hit), updating as the plan runs. A failed step offers <em>Retry</em> and <em>Skip</em> right there; <em>Open in Plans</em> shows it in the Plan window.</li>
  <li><strong>Manual:</strong> Imber proposes the run as a card with the steps and the Plans window opens on that plan; approve in the chat (<Kbd keys={['Ctrl', 'Enter']} size="sm" />) or run it from the window. <strong>Auto:</strong> it runs. What a plan does on the remote cannot be taken back; stopping it lets the steps in flight finish and runs <em>finally</em>.</li>
</ul>

<h2>Undo</h2>
<p>Everything Imber changes here is written to a log. <strong>Imber: Undo Last Action</strong> (palette), the undo button in the panel header, or asking Imber, takes back the newest reversible one; <strong>Imber: Actions…</strong> lists them all.</p>
<table class="shortcuts-table">
  <thead><tr><th>Can be undone</th><th>How</th></tr></thead>
  <tbody>
    <tr><td>A commit</td><td>HEAD moves back; the changes stay staged.</td></tr>
    <tr><td>A branch created, switched to or deleted</td><td>Back to the previous branch and the new one deleted / the old one recreated at its tip.</td></tr>
    <tr><td>Stage, unstage</td><td>The index is put back.</td></tr>
    <tr><td>A merge or pull that moved HEAD</td><td>A hard reset to where it was — after the usual recovery snapshot, and only with a clean tree.</td></tr>
  </tbody>
</table>
<p><strong>Cannot be undone:</strong> a push, a merge request opened, a plan run. They are in the list with the reason. Undo works newest first and checks the repository is still as Imber left it (HEAD in place); if not, it says so instead of dropping what you did since. Undoing always asks for approval, even in Auto mode, and never runs unattended.</p>

<h2>Cost and privacy</h2>
<ul>
  <li>The conversation's cost so far is shown under the message box; a per-conversation limit from the shared settings stops it when reached.</li>
  <li>What the model reads goes to the provider: diffs, commit messages, author names. The staged diff sent for a commit message is cut at a size budget, with the files left out named.</li>
</ul>

<h2>Notes</h2>
<ul>
  <li>Conversations are kept while Corvus runs; they are not saved to disk.</li>
  <li>Imber works on one repository at a time, the one in the active tab.</li>
</ul>
