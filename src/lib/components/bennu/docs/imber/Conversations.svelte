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
  <li><strong>Title</strong>: in its first reply Imber names the conversation after the task (a few words, in your language), and the tab follows at once. Afterwards it changes the title only when you ask it to; a title you set yourself stays yours.</li>
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
    <tr><td><strong>Tools</strong></td><td>All the families you left on</td><td>Four: the help about Bennu, opening a panel, drawing a diagram, showing a picture</td></tr>
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
<p>
  The thread follows new text while you are at its end; scroll up and it stays where you are, scroll back to the end and it follows again. Each conversation reopens where you left it — closing the panel or switching conversation and coming back lands at the same place, or at the end if that is where you were. Expanded over the editor (<kbd>Ctrl</kbd> + <kbd>Shift</kbd> + <kbd>U</kbd>) the thread is a wide column and the conversation and its context meter share one row.
</p>
<ul>
  <li>Your messages on the right; Imber's as flowing Markdown (headings, lists, tables, quotes, highlighted code) that you can select. Hover an answer to copy it or <strong>open it as a document</strong>, a Markdown editor where you can edit and save it without changing the conversation. Hover your own message to copy it.</li>
  <li>Tools it used, folded into one line that opens on demand ("Read 2 files · Searched once · 41s"). Inside, each call says how long it ran, always at the right edge (and, in amber, how long it waited for your answer on a card); its icon turns red when it failed or was refused, and hovering it shows the whole of what came back, and a <em>Model answered</em> row says how long each answer of the model took and how many tokens it wrote; hover it to see how much of that was waiting for the first token.</li>
  <li>Under a finished answer, where its time went: the total, then <em>model</em>, <em>tools</em> and <em>waiting for you</em> apart — the way to tell a slow tool from a model that writes a lot.</li>
  <li>Cards for proposals (Imber › Reviewing changes), questions, access prompts, the step plan, sub-agents, live runs and debugger stops.</li>
  <li>Notices in the thread: a compaction, a limit reached, an access grant, a system note about another conversation.</li>
</ul>

<h2>The step plan</h2>
<p>
  For work with several steps Imber writes a plan first and ticks the steps off. The card lists them (done, in progress, still to do) with a bar and "3 of 7 · 43%". The percentage is steps done over steps planned: it says how much of the list is done, not how long the rest will take, and it goes down if the plan grows. The plan costs a few hundred output tokens per update, a few cents for a task of several steps. Questions and small changes get no plan. <strong>Step plan</strong> in the composer's settings menu (and in Settings › Imber) switches it off; the tool and its instruction are then not sent at all.
 The plan is also part of what Imber reads back: after a stop, a <em>continue</em> or a compaction it still has the latest plan, word for word, and a new message in a conversation whose plan is still open reminds it where the plan got to. While it works, a plan left untouched for five tool calls is recalled to it with the step in progress, so steps are ticked as they are done; and if it ends its answer with steps still open, it is asked once to tick them or to say what is left.</p>

<h2>The strip on the message box</h2>
<p>
  A strip on top of the message box says what the conversation has going on, so you never scroll back to find a card. <strong>Plan</strong> shows the steps done (<em>3/6</em>) and names the step being worked on, with the plan's progress as a line along the strip's foot; <strong>Agents</strong> says how many are running, and <strong>Review</strong> how many proposals wait for your decision. Each segment is there only when it has something to show. Press one to open its panel over the thread — every step of the plan, with the one in progress brought to the middle and marked <em>Now</em>; every sub-agent with its Stop; every proposal with <em>Review…</em>. Press it again, Esc or a click outside closes it. The command palette opens the same panels (<em>Imber: Show the plan</em>, <em>Show the sub-agents</em>, <em>Show the edits to review</em>).
</p>

<h2>Writing while it works</h2>
<p>
  You do not have to wait or stop Imber to say something. While it works, write in the message box and press <kbd>Ctrl</kbd> + <kbd>Enter</kbd> (or the arrow beside Stop): the message waits above the box, marked <em>Waiting</em>, and Imber reads it before its next request — after the tool it is running, without stopping. In the thread it then stands where it was read, as <em>You, while it worked</em>. Words only: files go with your next ordinary message. If the run ends before reading it, the message comes back into the box for you to send.
</p>

<h2>Steps and non-stop</h2>
<p>
  A run makes at most <strong>Steps per run</strong> requests to the model (Settings › Imber › Conversations, 40 by default; a step is one request, however many tools it calls). When it gets there it stops with “Stopped after N steps” and waits for you to say <em>continue</em>, which picks up where it was.
</p>
<p>
  <strong>Non-stop</strong>, in the composer's settings menu or the command palette (<em>Imber: Non-stop in this conversation</em>), lifts that cap for one conversation: its runs go on until the work is done or you stop them. The spending limits still stop it, and a scheduled run in that conversation keeps the cap. The choice is kept with the conversation. While it is on, the settings button carries a small ∞ and the menu's row reads <em>On</em>.
</p>

<h2>Several conversations at once</h2>
<p>
  Several conversations can work at the same time: start one, open another and send — each has its own run, its own status line and its own Stop, and the conversation list marks the ones working with <em>Working</em>. Two conversations can run on the same project. A file one of them is changing is reserved to it until its run ends: the other is refused, naming the conversation, and can wait or work elsewhere (reading is never blocked). Services and debug sessions belong to the conversation that started them. Builds queue. See Imber › Running and observing.
</p>
