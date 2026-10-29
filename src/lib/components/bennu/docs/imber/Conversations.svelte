<span class="eyebrow">Imber</span>
<h1>Imber: Conversations</h1>

<p class="doc-lead">
  Every conversation with Imber is saved on disk, listed by the title in the panel header, and is either an agent conversation (works in the project) or a chat (talks only).
</p>

<h2>The conversation switcher</h2>
<p>
  The title in the header is a switcher. It lists your conversations, today's first, each with the model it used, when it was last touched and what it cost. The open one shows as saved. Choosing a row opens that conversation, with its thread and its context meter.
</p>
<ul>
  <li><strong>New conversation</strong>: the pencil-and-square button in the header, or the command <em>Imber: New conversation in an isolated copy</em> for a conversation in a copy.</li>
  <li><strong>Rename</strong>: the pencil next to the title.</li>
  <li><strong>Delete</strong>: the trash next to the title, or <em>Delete conversation</em> in the <strong>…</strong> menu. It asks first. Deleting a conversation also removes its isolated copy, its index and the processes started in it. The money it cost stays counted against your limits: costs are kept in a ledger of their own.</li>
</ul>
<p>
  Conversations are plain files in Bennu's data folder (<code>arbor/data/bennu/assistant</code>), one per conversation, holding both what the panel draws and what is sent to the model next time.
</p>

<h2>Agent or Chat</h2>
<p>
  The <strong>Agent / Chat</strong> switch in the header chooses between two kinds of conversation. Each has its own list and its own home screen, and a conversation is one or the other for good; opening a chat from the list switches the panel to Chat.
</p>
<table>
  <thead><tr><th></th><th>Agent</th><th>Chat</th></tr></thead>
  <tbody>
    <tr><td><strong>Works in the project</strong></td><td>Yes: reads, searches, edits, builds, runs, debugs through Bennu's tools</td><td>No. It cannot read, search or change anything</td></tr>
    <tr><td><strong>What it sees</strong></td><td>The project, the open file, the selection, the instruction files</td><td>Only what you type or select</td></tr>
    <tr><td><strong>Tools</strong></td><td>All the families you left on</td><td>Three: the help about Bennu, opening a panel, drawing a diagram</td></tr>
    <tr><td><strong>Modes and reach</strong></td><td>Mode menu and Reach apply</td><td>Not shown: nothing to ask</td></tr>
    <tr><td><strong>Good for</strong></td><td>Anything about the project</td><td>A question unrelated to the project, or when you want to be sure nothing is touched</td></tr>
  </tbody>
</table>
<p>
  <strong>Chat only</strong> in Settings › Imber › New conversations makes new conversations chats by default. Git, skills and sub-agents are not available in a chat.
</p>

<h2>The home screen</h2>
<p>
  With no conversation open the panel shows its home. An agent home offers six starters about the project (explain this file, explain the project, show me a panel, find why it fails, document it, review my changes); a chat home offers four that need no project (explain a concept, write a regex, rewrite a text, think it through). It also lists your four most recent conversations and the month's spending so far.
</p>

<h2>What is in the thread</h2>
<ul>
  <li>Your messages on the right; Imber's as flowing Markdown (headings, lists, tables, quotes, highlighted code) that you can select. Hover an answer to copy it or <strong>open it as a document</strong>, a Markdown editor where you can edit and save it without changing the conversation. Hover your own message to copy it.</li>
  <li>Tools it used, folded into one line that opens on demand ("Read 2 files · Searched once").</li>
  <li>Cards for proposals (Imber › Reviewing changes), questions, access prompts, the step plan, sub-agents, live runs and debugger stops.</li>
  <li>Notices in the thread: a compaction, a limit reached, an access grant, a system note about another conversation.</li>
</ul>

<h2>The step plan</h2>
<p>
  For work with several steps Imber writes a plan first and ticks the steps off. The card lists them (done, in progress, still to do) with a bar and "3 of 7 · 43%". The percentage is steps done over steps planned: it says how much of the list is done, not how long the rest will take, and it goes down if the plan grows. The plan costs a few hundred output tokens per update, a few cents for a task of several steps. Questions and small changes get no plan. <strong>Step plan</strong> in the composer's settings menu (and in Settings › Imber) switches it off; the tool and its instruction are then not sent at all.
</p>

<h2>Several conversations at once</h2>
<p>
  Two conversations can run on the same project. A file one of them is changing is reserved to it until its run ends: the other is refused, naming the conversation, and can wait or work elsewhere (reading is never blocked). Services and debug sessions belong to the conversation that started them. Builds queue. See Imber › Running and observing.
</p>
