<span class="eyebrow">Imber</span>
<h1>Imber: Permissions and modes</h1>

<p class="doc-lead">
  Two settings decide what Imber may do without asking: the <strong>mode</strong> (how much it decides for itself) and the <strong>reach</strong> (where it may go). The table below answers "what asks, when" for every action.
</p>

<h2>The three modes</h2>
<p>
  Chosen from the mode menu in the composer, with <kbd>Ctrl</kbd> + <kbd>Shift</kbd> + <kbd>Q</kbd> (next mode, while focus is in the panel), from the Command Palette (<em>Imber: Mode — …</em>, <em>Imber: Next mode</em>) and, as the default for new conversations, in Settings › Imber › New conversations › Mode.
</p>
<ul>
  <li><strong>Manual</strong>: every edit is a proposal you approve (review the diff, apply or reject). Deleting and building ask. Anything outside what you allowed asks.</li>
  <li><strong>Accept edits</strong>: edits and creations inside what you allowed are applied as they are proposed, and each can be undone from its card. Deleting inside the reach does not ask. The project's own build and tests ask once per run.</li>
  <li><strong>Autonomous</strong>: the same as Accept edits, plus no question for the project's own build and tests, plus one extra power: it may open for reading and writing a file that an error of its own build names (see below).</li>
</ul>
<p>
  A saved mode named <em>Auto</em> from an older setup reads as Accept edits.
</p>

<h2>The four reaches</h2>
<p>
  The shield beside <em>Add context</em>, or Reach in the composer's settings menu, sets where Imber may read and change without asking. The default for new conversations is in Settings › Imber › What it may reach.
</p>
<table>
  <thead><tr><th>Reach</th><th>What it covers</th></tr></thead>
  <tbody>
    <tr><td><strong>This file</strong></td><td>Only the file you are in. No listing and no creating beside it. A tool that answers about the whole project (usages, callers, the framework catalogue) is held to the project folder, so it asks.</td></tr>
    <tr><td><strong>This project</strong> (default)</td><td>Every file and every Bennu tool of the project.</td></tr>
    <tr><td><strong>All open projects</strong></td><td>The projects open in the workspace, the one you are in first, up to Settings › Imber › Projects at once (6 by default).</td></tr>
    <tr><td><strong>Anywhere on disk</strong></td><td>No access prompts at all. Reading a secret, deleting and the hard limits below still apply.</td></tr>
  </tbody>
</table>
<p>
  Always allowed on top of the reach: the <strong>extra folders</strong> you added in Settings › Imber › What it may reach, and its own <strong>Temp area</strong> (scratch files and downloaded logs) when <em>Use the Temp folders</em> is on. <strong>Edit files</strong> off makes Imber read-only: every write is refused with "editing is switched off".
</p>

<h2>What asks, when</h2>
<p>
  "Inside" means inside the reach, the extra folders or the Temp area. Cells say what happens; <em>asks</em> means a yes/no prompt in the thread, <em>proposal</em> means a card you review.
</p>
<table>
  <thead><tr><th>Action</th><th>Manual</th><th>Accept edits</th><th>Autonomous</th></tr></thead>
  <tbody>
    <tr><td><strong>Read, search, project intelligence (inside)</strong></td><td>Free</td><td>Free</td><td>Free</td></tr>
    <tr><td><strong>Edit an existing file (inside)</strong></td><td>Proposal</td><td>Applied at once, Undo on the card</td><td>Applied at once, Undo on the card</td></tr>
    <tr><td><strong>Create a file (inside)</strong></td><td>Proposal</td><td>Applied at once</td><td>Applied at once</td></tr>
    <tr><td><strong>Rename a symbol, add a parameter, replace across files, structural replace, restore from Local History</strong></td><td>Proposal</td><td>Applied, recorded in Local History</td><td>Applied, recorded in Local History</td></tr>
    <tr><td><strong>Delete (inside the reach)</strong></td><td>Asks</td><td>Free, but a call removing more than 50 files asks</td><td>Same as Accept edits</td></tr>
    <tr><td><strong>Delete in the Temp area</strong></td><td>Free</td><td>Free</td><td>Free</td></tr>
    <tr><td><strong>Delete outside the reach, or a whole project or allowed folder</strong></td><td>Asks</td><td>Asks</td><td>Asks</td></tr>
    <tr><td><strong>Build or run the tests of the project (<code>bennu_build</code>, <code>bennu_test_run</code>)</strong></td><td>Asks on every call</td><td>Asks once per run</td><td>Free</td></tr>
    <tr><td><strong>Other things that execute code (cargo run, npm scripts, run configurations, launching a program)</strong></td><td>Asks</td><td>Asks</td><td>Asks</td></tr>
    <tr><td><strong>Other Bennu tools that change something</strong></td><td>Asks</td><td>Free</td><td>Free</td></tr>
    <tr><td><strong>Start a process (<code>run_start</code>)</strong></td><td>Asks every time</td><td>Asks every time</td><td>Asks every time</td></tr>
    <tr><td><strong>Run a recipe or a scenario</strong></td><td>Asks once per content of the recipes file or scenario</td><td>Same</td><td>Same</td></tr>
    <tr><td><strong>HTTP request to localhost</strong></td><td>GET, HEAD, OPTIONS free; any other method asks unless the port belongs to a process it started</td><td>Same</td><td>Same</td></tr>
    <tr><td><strong>Debugger: set breakpoints, read the stack and values, step</strong></td><td>Free</td><td>Free</td><td>Free</td></tr>
    <tr><td><strong>Debugger: launch or attach</strong></td><td>Asks every time</td><td>Asks every time</td><td>Asks every time</td></tr>
    <tr><td><strong>Debugger: change the running program (throw, return early, drop a frame, set a value)</strong></td><td>Off by default; when on, asks once per conversation</td><td>Same</td><td>Same: Autonomous does not skip it</td></tr>
    <tr><td><strong>Bevy: read a running game</strong></td><td>Free (this machine only)</td><td>Free</td><td>Free</td></tr>
    <tr><td><strong>Bevy: change the running game</strong></td><td>Off by default; when on, asks once per conversation</td><td>Same</td><td>Same</td></tr>
    <tr><td><strong>Screenshot of a localhost page</strong></td><td>Free</td><td>Free</td><td>Free</td></tr>
    <tr><td><strong>Git: read (status, diff, log, blame)</strong></td><td>Free once git is switched on</td><td>Free</td><td>Free</td></tr>
    <tr><td><strong>Git: stage, unstage</strong></td><td>Asks</td><td>Free</td><td>Free</td></tr>
    <tr><td><strong>Git: commit</strong></td><td>Asks</td><td>Asks</td><td>Asks. Push does not exist</td></tr>
    <tr><td><strong>A path outside the reach</strong></td><td>One prompt: Deny, Allow reading, Allow reading and writing, for that folder, for the session</td><td>Same</td><td>Same, except the build-error case below. "Anywhere on disk" never asks</td></tr>
    <tr><td><strong>A file named by an error of Imber's own build or check (inside the project)</strong></td><td>The prompt says the build reported an error there</td><td>Same</td><td>Opened for reading and writing without a prompt, for that run, shown with a Revoke button</td></tr>
    <tr><td><strong>Read a secret file (<code>.env</code>, keys)</strong></td><td>Asks every time</td><td>Asks every time</td><td>Asks every time</td></tr>
    <tr><td><strong>Write a secret file, or anything under <code>.git</code></strong></td><td>Refused</td><td>Refused</td><td>Refused</td></tr>
    <tr><td><strong>Ask you a question (<code>ask_user</code>)</strong></td><td>Always possible; see Imber › Decisions and questions</td><td>Same</td><td>Same</td></tr>
  </tbody>
</table>
<p>
  A grant you give (reading, or reading and writing) lasts for the session and applies to that folder. A sub-agent's grants are its own and do not carry back to the conversation; see Imber › Sub-agents.
</p>

<h2>Autonomous and the build-error files</h2>
<p>
  Under Autonomous, a file named by an <em>error</em> of a build or check Imber ran itself (<code>bennu_build</code>, <code>bennu_check_file</code>, <code>bennu_problems</code>) and located inside the project you are in is opened for reading and writing, for that run only, without a prompt. Limits: at most 25 files a run; never anything under build output or fetched code (<code>target</code>, <code>build</code>, <code>out</code>, <code>dist</code>, <code>node_modules</code>, generated folders); never a dependency manifest or lockfile (<code>pom.xml</code>, <code>Cargo.toml</code>, <code>package.json</code>); never a secret. The thread shows each one ("Access granted to … because the build reports an error there") with a <em>Revoke</em> button, and the access ends with the run. Files of another project open this way only when the reach is <em>all open projects</em>; otherwise it asks. In Manual and Accept edits the prompt for such a file says why it is being asked.
</p>

<h2>Revoking access</h2>
<p>
  Press <em>Revoke</em> on an access card to take back a folder before the session ends. Changing the reach to a narrower one takes effect from the next call. Ending the run ends the Autonomous grants.
</p>

<h2>Hard limits no mode changes</h2>
<ul>
  <li>No shell. Imber cannot run an arbitrary command; processes start only through <code>run_start</code> (asks), recipes and scenarios (ask), or the project's own build and test commands.</li>
  <li>Secret files (<code>.env</code>, keys) are never written; reading one asks every time.</li>
  <li><code>.git</code> is never changed by file tools.</li>
  <li>Deleting the project folder itself, an allowed folder, or anything outside the reach always asks.</li>
  <li>Push does not exist; a commit always asks.</li>
  <li>Spending limits apply in every mode (Imber › Cost and context).</li>
</ul>
