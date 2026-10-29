<span class="eyebrow">Imber</span>
<h1>Imber: Decisions and questions</h1>

<p class="doc-lead">
  The decision level sets how much Imber chooses alone when a choice is yours; when it does ask, it asks with a card of choices in the thread.
</p>

<h2>The decision level</h2>
<table>
  <thead><tr><th>Level</th><th>Behaviour</th></tr></thead>
  <tbody>
    <tr><td><strong>Ask at forks</strong> (default)</td><td>It stops to ask when a choice is yours.</td></tr>
    <tr><td><strong>Only big decisions</strong></td><td>It asks about architecture, new dependencies and anything hard to undo, and decides the rest.</td></tr>
    <tr><td><strong>Decide itself</strong></td><td>It picks the best option, asks only when truly blocked (at most two questions a run) and lists its decisions at the end.</td></tr>
  </tbody>
</table>
<p>
  Set the default in <strong>Settings › Imber › New conversations › Decisions</strong>. A conversation can follow the setting or have its own level: composer settings menu › <em>Decisions</em>, or <em>Imber: Decisions in this conversation…</em> in the Command Palette. A change applies from the next message.
</p>

<h2>Your own words about decisions</h2>
<p>
  Under the level there is a free-text field ("Your instructions about decisions", up to 2,000 characters) for your own rules at any level, for example "always ask before touching public API" or "never add a dependency". A conversation can add its own text in the same window; it is appended to the global one.
</p>
<p>
  The same window also holds the conversation's own switch for <em>Start a run when the program stops</em> (see Imber › Debugging with Imber).
</p>

<h2>Questions</h2>
<p>
  Where it would otherwise guess, Imber asks with <code>ask_user</code>. Each question is a column of choices, one per row: pick one, or tick several, and <em>Other…</em> is always there to type your own. When it asks several things at once, a strip of tabs names them (a tick once answered): jump to any, or use <strong>Back</strong> and <strong>Next</strong>; the last step sends all the answers together, so changing an earlier one costs nothing. Answered, the card shrinks to what you decided and stays in the thread.
</p>
<p>
  Questions about access or confirmation ("Allow this?", "Start a process?") are not part of this: they follow the mode and reach, see Imber › Permissions and modes. Sub-agents cannot ask you questions.
</p>
