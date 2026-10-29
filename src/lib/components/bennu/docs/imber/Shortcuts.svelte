<span class="eyebrow">Imber</span>
<h1>Imber: Keyboard and palette</h1>

<p class="doc-lead">
  Everything Imber does can be driven from the keyboard: a handful of shortcuts, and a family of Command Palette verbs that all start with "Imber:".
</p>

<h2>Shortcuts</h2>
<table>
  <thead><tr><th>Keys</th><th>Where</th><th>What it does</th></tr></thead>
  <tbody>
    <tr><td><kbd>Alt</kbd> + <kbd>A</kbd></td><td>Bennu window</td><td>Open or close the Imber panel.</td></tr>
    <tr><td><kbd>Ctrl</kbd> + <kbd>Enter</kbd></td><td>Composer</td><td>Send the message. <kbd>Enter</kbd> is a new line.</td></tr>
    <tr><td><kbd>Ctrl</kbd> + <kbd>Shift</kbd> + <kbd>U</kbd></td><td>Bennu window, panel on screen</td><td>Expand the panel over the editor, or restore it. <kbd>Esc</kbd> inside the panel (with nothing open over it) also restores it.</td></tr>
    <tr><td><kbd>Ctrl</kbd> + <kbd>Shift</kbd> + <kbd>Q</kbd></td><td>Focus inside the panel</td><td>Next mode: Manual, Accept edits, Autonomous.</td></tr>
    <tr><td><kbd>Ctrl</kbd> + <kbd>Shift</kbd> + <kbd>Enter</kbd></td><td>Focus inside the panel, isolated copy active</td><td>Review and apply the isolated copy.</td></tr>
    <tr><td><kbd>Ctrl</kbd> + <kbd>Shift</kbd> + <kbd>V</kbd></td><td>Composer</td><td>Paste exactly what is on the clipboard, with no code fence.</td></tr>
    <tr><td><kbd>/</kbd></td><td>Start of the composer</td><td>Pick one of the enabled skills.</td></tr>
    <tr><td><kbd>↑</kbd> / <kbd>↓</kbd></td><td>Review window</td><td>Move between the files of a proposal.</td></tr>
    <tr><td><kbd>Ctrl</kbd> + <kbd>Enter</kbd></td><td>Review window</td><td>Apply all (or, on an outdated proposal, re-apply on the current file).</td></tr>
  </tbody>
</table>

<h2>Command Palette</h2>
<table>
  <thead><tr><th>Verb</th><th>Shown when</th></tr></thead>
  <tbody>
    <tr><td><em>Toggle Imber</em></td><td>always</td></tr>
    <tr><td><em>Ask Imber about this…</em></td><td>in an editor: opens the margin menu at the caret</td></tr>
    <tr><td><em>Imber: Expand panel</em> / <em>Imber: Restore panel</em></td><td>depending on the state of the panel</td></tr>
    <tr><td><em>Imber: Retry the failed request</em></td><td>when the last answer ended in an error</td></tr>
    <tr><td><em>Imber: Retry now (skip the wait)</em></td><td>while a request waits to be sent again</td></tr>
    <tr><td><em>Imber: Manage skills…</em></td><td>always</td></tr>
    <tr><td><em>Imber: Mode — Manual</em> / <em>Accept edits</em> / <em>Autonomous</em></td><td>for the modes you are not in</td></tr>
    <tr><td><em>Imber: Next mode (Manual → Accept edits → Autonomous)</em></td><td>always</td></tr>
    <tr><td><em>Imber: Decisions in this conversation…</em></td><td>always</td></tr>
    <tr><td><em>Imber: Sub-agents in this conversation — next</em></td><td>in an agent conversation</td></tr>
    <tr><td><em>Imber: Open the latest sub-agent's thread…</em></td><td>when the conversation has sub-agents</td></tr>
    <tr><td><em>Imber: Stop all sub-agents</em></td><td>while sub-agents are running</td></tr>
    <tr><td><em>Imber: New conversation in an isolated copy (text only)</em></td><td>always</td></tr>
    <tr><td><em>Imber: New conversation in a git worktree (indexed)</em></td><td>always</td></tr>
    <tr><td><em>Imber: Review and apply the isolated copy</em> (two entries, one with the shortcut and one that also shows the panel)</td><td>while a copy is active</td></tr>
    <tr><td><em>Imber: Discard the isolated copy…</em></td><td>while a copy is active</td></tr>
  </tbody>
</table>
<p>
  Shortcuts that are not Imber's own but matter when working with it: <kbd>Ctrl</kbd> + <kbd>Shift</kbd> + <kbd>F8</kbd> opens the breakpoints list (the ones Imber set are badged), and <kbd>Ctrl</kbd> + <kbd>Shift</kbd> + <kbd>F11</kbd> opens Live.
</p>
<p>
  Without a mouse: open the panel with <kbd>Alt</kbd> + <kbd>A</kbd>, type, <kbd>Ctrl</kbd> + <kbd>Enter</kbd>; review with the keys above; change mode with <kbd>Ctrl</kbd> + <kbd>Shift</kbd> + <kbd>Q</kbd> or the palette verbs above.
</p>
