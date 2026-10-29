<span class="eyebrow">Imber</span>
<h1>Imber: Sub-agents</h1>

<p class="doc-lead">
  A sub-agent is a helper Imber starts with a clean context: it knows only its task, works with a narrower set of tools, and sends back only a summary, so what it reads never fills the conversation's own window.
</p>

<h2>When to use them</h2>
<p>
  They are for independent side tasks, for exploration that would read many files to answer one question, and for work unrelated to what the conversation was doing. They are not for anything a call or two answers.
</p>

<h2>Turning them on</h2>
<p>
  Off by default. <strong>Settings › Imber › New conversations › Sub-agents</strong> turns them on for every conversation. In the composer's settings menu, <em>Sub-agents</em> (or the palette entry <em>Imber: Sub-agents in this conversation — next</em>) cycles through <em>Follow the settings</em>, <em>On</em> and <em>Off</em> for one conversation, applying from its next message. When they are off Imber is not even told they exist. Not available in a chat.
</p>

<h2>What an agent can do</h2>
<ul>
  <li><strong>Read-only by default.</strong> An agent reads and searches on the cheaper side model (Settings › Imber › Cheaper model for side jobs), and up to three run at once. Imber can go on while they work and collects their summaries later (<code>spawn_agent</code>, <code>await_agents</code>).</li>
  <li><strong>Editing</strong> is possible only when the conversation's own edits are applied rather than proposed (Accept edits or Autonomous, with editing on). Then Imber waits for the agent, so only one of them writes at a time, and file reservations apply as for any conversation.</li>
  <li><strong>Limits per agent:</strong> 25 steps, one level of agents (an agent cannot start agents), a summary of at most 6,000 characters.</li>
</ul>

<h2>Never more than Imber</h2>
<p>
  An agent's access is the conversation's, narrowed. It cannot reach anything Imber could not, never deletes without asking, does not inherit files opened for Imber's own run, and under Autonomous it runs as Accept edits: it does not open files by itself. Anything beyond what it has is asked of you, in the conversation's thread, labelled with the agent's name; what you allow it is the agent's alone. It cannot ask you questions, start agents or write to other conversations.
</p>

<h2>Cost</h2>
<p>
  What an agent spends is added to the conversation and counts against the spending limits like Imber's own requests. Each agent also has a cap of its own, a third of the session limit and at most €1, and stops when it reaches it. Stopping the conversation's run stops its agents.
</p>

<h2>Seeing them</h2>
<ul>
  <li>Above the thread an <em>Agents</em> list shows each agent's state, how long it has run and what it cost.</li>
  <li>The title opens its thread, read-only and live (<em>Imber: Open the latest sub-agent's thread…</em>). It reads as a chat between the two: the task Imber (badged <em>main</em>) gave it, then the agent's own messages (badged <em>sub-agent</em>), a writing mark while it works and, at the end, a line saying it handed its summary back, stopped or failed. The view follows the end of the thread unless you scrolled up.</li>
  <li>Each agent has a Stop; <em>Stop all</em> and <em>Imber: Stop all sub-agents</em> stop them together.</li>
  <li>A card for each stays in the thread: what it was asked, what it may do, its state, time and cost, and the start of its answer. While the agent runs the card's border moves. An agent's record and thread are kept on disk for as long as its conversation keeps them, so the Agents list and an agent's thread still open after a restart; an agent that was running when the app closed shows as stopped. What it read is not copied: the kept thread says <em>read this file</em>, not the file. Imber can continue a finished agent with <code>resume_agent</code> and a new instruction: it remembers what it concluded and reads again what it needs. The trash button on a finished agent deletes it; compacting the conversation, or deleting it, deletes them all.</li>
</ul>

<h2>A hint, not a decision</h2>
<p>
  When the window is filling up (from 70% at medium effort, 50% at high; never at low) and your new message looks unrelated to the files worked on so far, a one-line note is added at the end of that message suggesting an agent. Imber decides.
</p>
