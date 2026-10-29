<span class="eyebrow">Imber</span>
<h1>Imber: the assistant in Bennu</h1>

<p class="doc-lead">
  Imber is the AI assistant built into Bennu: a chat and agent panel on the right of the editor that reads, searches, edits, builds, runs and debugs your project through Bennu's own tools, and asks before anything that matters.
</p>

<h2>What Imber is</h2>
<p>
  Imber runs on Claude, a model made by Anthropic, and talks to it with your own Anthropic API key. It is not a separate product: it lives in the Bennu window (<kbd>Alt</kbd> + <kbd>A</kbd>, or the sparkle on the right rail) and works on the project that is open. The name is Latin for <em>rain</em>: the thing the whole forest of Arbor runs on.
</p>
<p>
  There are two kinds of conversation, kept apart by the <strong>Agent / Chat</strong> switch in the panel header. An <em>agent</em> conversation works inside the project with Bennu's tools. A <em>chat</em> conversation has no tools on the project at all: it only sees what you type or select, and can look up Bennu's own documentation, open a panel and draw a diagram.
</p>

<h2>The ideas behind it</h2>
<ul>
  <li><strong>Bennu's tools, never a shell.</strong> Imber reads and changes the project through the same engine you use: the index, usages, rename, structural replace, build, tests, Local History. It cannot type arbitrary commands.</li>
  <li><strong>You decide how much it decides.</strong> Three modes (Manual, Accept edits, Autonomous), four reaches (this file, this project, all open projects, anywhere on disk) and a matrix of what asks when. See Imber › Permissions and modes.</li>
  <li><strong>Everything it changes can be reviewed and undone.</strong> Proposals, a diff window, Undo on applied changes, Local History, and an isolated copy for risky work. See Imber › Reviewing changes.</li>
  <li><strong>The cost is visible and capped.</strong> Every conversation shows what it cost, and limits in euros stop a runaway run. See Imber › Cost and context.</li>
  <li><strong>Your key stays on your machine.</strong> It lives in a credential vault of its own and goes only to Anthropic. See Imber › Privacy and safety.</li>
</ul>

<h2>Where to read next</h2>
<table>
  <thead><tr><th>Page</th><th>Read it to learn</th></tr></thead>
  <tbody>
    <tr><td><strong>Imber › Getting started</strong></td><td>Opening the panel, setting the key, choosing a model, sending a first request.</td></tr>
    <tr><td><strong>Imber › Conversations</strong></td><td>History, renaming, deleting, Agent versus Chat, the home screen, the step plan.</td></tr>
    <tr><td><strong>Imber › Permissions and modes</strong></td><td>The single table of what asks, by mode and by action.</td></tr>
    <tr><td><strong>Imber › Reviewing changes</strong></td><td>Proposals, outdated proposals, Undo, Local History, the isolated copy.</td></tr>
    <tr><td><strong>Imber › Tools it can use</strong></td><td>Every family of tools, how to switch one off, git, several projects.</td></tr>
    <tr><td><strong>Imber › Running and observing</strong></td><td>Processes, recipes, scenarios, logs, the build queue and who broke the build.</td></tr>
    <tr><td><strong>Imber › Debugging with Imber</strong></td><td>Breakpoints, stops, snapshots and the tools that change a running program.</td></tr>
    <tr><td><strong>Imber › Bevy games and the profiler</strong></td><td>Looking into a running Bevy game and reading a profiler recording.</td></tr>
    <tr><td><strong>Imber › Screenshots and diagrams</strong></td><td>Seeing a front end on localhost, and drawing Mermaid diagrams.</td></tr>
    <tr><td><strong>Imber › Skills</strong></td><td>Teaching Imber how you want a kind of task done.</td></tr>
    <tr><td><strong>Imber › Sub-agents</strong></td><td>Helpers with a clean context for side tasks.</td></tr>
    <tr><td><strong>Imber › Decisions and questions</strong></td><td>How much it decides alone and how it asks you.</td></tr>
    <tr><td><strong>Imber › Cost and context</strong></td><td>Tokens, the cache, compaction, limits in euros.</td></tr>
    <tr><td><strong>Imber › Working from the editor</strong></td><td>The sparkle in the margin, comments, stack traces, pasted code, the expanded panel.</td></tr>
    <tr><td><strong>Imber › Settings reference</strong></td><td>Every setting: what it does, its default, where it is stored.</td></tr>
    <tr><td><strong>Imber › Keyboard and palette</strong></td><td>Shortcuts and Command Palette verbs.</td></tr>
    <tr><td><strong>Imber › Privacy and safety</strong></td><td>What leaves the machine, what is masked, what is never allowed.</td></tr>
    <tr><td><strong>Imber › Troubleshooting</strong></td><td>Common messages and what to do about them.</td></tr>
  </tbody>
</table>

<h2>Pages in other parts of the manual</h2>
<p>
  Some topics belong to a feature and live with it: <strong>Live</strong> (processes, recipes, scenarios, the timeline and the build queue), <strong>Debugging</strong>, <strong>Bevy Remote</strong> and <strong>Profiler</strong>. Each has a short section for Imber and the Imber pages above link to them. <strong>Voice</strong> (in the Editor group) covers dictating to Imber by voice, the voice overlay, the speech models and the wake word.
</p>
