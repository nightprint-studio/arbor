<span class="eyebrow">Imber</span>
<h1>Imber: Privacy and safety</h1>

<p class="doc-lead">
  Imber sends to Anthropic what it reads in order to answer; the API key, secret files and other people's names get special treatment, and a short list of things is never allowed whatever you click.
</p>

<h2>What leaves your machine</h2>
<p>
  Each request goes to Anthropic's API, over the key you stored, and nothing goes anywhere else. A request contains:
</p>
<ul>
  <li>Imber's instructions and the definitions of the tools it was offered;</li>
  <li>your project instruction files (<code>CLAUDE.md</code>, <code>AGENTS.md</code>, your own instructions file) if that setting is on, and the one-line index of the skills you enabled;</li>
  <li>the whole conversation: your messages, Imber's answers and the results of every tool it called;</li>
  <li>the open file's path and the lines you selected, with your message.</li>
</ul>
<p>
  <strong>Tool results are the part to keep in mind.</strong> A file Imber reads, a search result, build and test output, logs, a debugger value, a Bevy component or a page screenshot is part of the conversation and is sent, and sent again whenever the conversation is re-read. Files it does not read are not sent. In a chat only what you type, paste or select is sent.
</p>
<p>
  To see exactly what goes, use <em>Show what would be sent…</em> or <em>Dry run</em> in the panel's <strong>…</strong> menu: both show the request body and its size, with the key masked.
</p>

<h2>The API key</h2>
<p>
  Stored in a credential vault of its own, apart from the one holding your git tokens; never written to a config file; never held by the frontend after you typed it (the Settings page only learns whether one is stored); read only by the backend when a request is made and sent only to Anthropic.
</p>

<h2>What is masked or replaced</h2>
<ul>
  <li><strong>Authors in git results</strong>: with Hide authors on (the default) people appear as ids such as <code>Author-7f3a9c</code>. The mapping stays on this machine and is used to turn ids back into names for you.</li>
  <li><strong>Debugger values</strong> of variables named like secrets (password, token, secret, apiKey, credential…) are masked before Imber sees them.</li>
  <li><strong>Secret files</strong> (<code>.env</code>, key files) are not read without your yes each time.</li>
  <li><strong>The key</strong> in the request preview.</li>
</ul>
<p>
  Masking is by name and pattern. It is not a guarantee that a secret pasted into a message, or sitting inside a file with an ordinary name, is hidden: what you ask Imber to read, it reads.
</p>

<h2>What is never granted</h2>
<ul>
  <li>A shell, or any command Imber typed itself.</li>
  <li>Writing a secret file, or anything under <code>.git</code>.</li>
  <li>Pushing to a remote. A commit always asks.</li>
  <li>Reaching anything but this machine over HTTP, for requests and screenshots and the Bevy connection.</li>
  <li>Deleting a whole project or allowed folder, or anything outside the reach, without asking.</li>
  <li>Changing a running program or Bevy game without the separate switch and a yes per conversation. Autonomous mode does not waive either.</li>
  <li>Running the commands inside a skill: skills are text.</li>
</ul>

<h2>Safeguards you can lean on</h2>
<ul>
  <li>Spending limits stop runaway runs in every mode.</li>
  <li>Edits are all-or-nothing, can be undone, and are recorded in Local History.</li>
  <li>Concurrent conversations cannot write the same file at once.</li>
  <li>Sub-agents never have more access than the conversation, and cannot start agents.</li>
  <li>An isolated copy keeps every change away from your files until you apply it.</li>
</ul>
<p>
  The Bennu help Imber consults is built into the application; looking things up in it sends nothing by itself, though the answer becomes part of the conversation.
</p>
