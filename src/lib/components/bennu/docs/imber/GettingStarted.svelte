<script lang="ts">
  import Callout from '$lib/components/shared/ui/Callout.svelte';
</script>

<span class="eyebrow">Imber</span>
<h1>Imber: Getting started</h1>

<p class="doc-lead">
  Open the panel with <kbd>Alt</kbd> + <kbd>A</kbd>, store your Anthropic API key once in Settings, pick a model, and send a request.
</p>

<h2>Open the panel</h2>
<p>
  The panel sits on the right of the editor. Open it with <kbd>Alt</kbd> + <kbd>A</kbd>, the sparkle on the right rail, or <em>Toggle Imber</em> in the Command Palette. With no conversation open it shows its <strong>home</strong>: the mode you are in, the project, the file and the instruction files it would work with, a few starters (an agent card sends a request at once; a chat card leaves the start of a sentence in the composer) and your recent conversations. The panel opens on it.
</p>

<h2>Set the API key</h2>
<Callout variant="info" title="You need an Anthropic API key">
  Imber is billed to your own Anthropic account. Every message costs money, and the conversation shows what it has cost.
</Callout>
<ol>
  <li>Open <strong>Settings › Imber</strong> (the <strong>…</strong> menu of the panel has <em>Imber settings…</em>).</li>
  <li>Paste the key under <strong>Connection › API key</strong> and press <strong>Save</strong>. The page then shows <em>Set</em>; it never shows the key again.</li>
  <li><strong>Remove</strong> deletes it from the vault.</li>
</ol>
<p>
  The key is kept in a credential vault of its own, apart from the one that holds your git tokens, is never written to a config file, and is sent only to Anthropic with each request. Without a key a request fails with "No API key is set. Add one in Settings › Imber."
</p>

<h2>Choose a model, effort and window</h2>
<p>The composer has one quiet line under the text box:</p>
<ul>
  <li><strong>Model</strong>: each entry shows what it is for, the size of its window and its price per million tokens (input / output). The list and the prices come from a file; see Imber › Cost and context.</li>
  <li><strong>Settings</strong> (the sliders icon): <em>Effort</em> (how hard it thinks: Low, Medium, High, Extra high, Max; a model without such a setting ignores it), <em>Context window</em> (the largest window Imber may use, 50K up to 1M, limited by the model), <em>Reach</em>, <em>Permissions</em>, <em>Decisions</em>, <em>Skills…</em>, <em>Step plan</em>, <em>Sub-agents</em>, <em>Isolated copy</em> and <em>Build isolation</em>. Each shows its current value.</li>
  <li><strong>Mode</strong> (agent conversations only): Manual, Accept edits or Autonomous. See Imber › Permissions and modes.</li>
</ul>
<p>
  The defaults for a <em>new</em> conversation are set in Settings › Imber › New conversations: Opus 5.5 at High effort with a 200K window, in Manual mode with the reach "this project". The composer changes them for the conversation you are in. The full list is in Imber › Settings reference.
</p>

<h2>Send a first request</h2>
<ol>
  <li>Open a project (Imber works inside one) and the file you care about. The file you are in and the lines you selected travel with the message, shown as chips above the text.</li>
  <li>Type a request, for example "Explain what this class does" or "Rename <code>total</code> to <code>amountDue</code> everywhere".</li>
  <li>Press <kbd>Ctrl</kbd> + <kbd>Enter</kbd> to send; <kbd>Enter</kbd> is a new line.</li>
  <li><strong>Stop</strong> (the square where Send was) ends a run; what it had written stays.</li>
</ol>
<p>
  In Manual mode a change arrives as a proposal card; open <em>Review</em> to read the diff and <strong>Apply all</strong> (<kbd>Ctrl</kbd> + <kbd>Enter</kbd> in the review window) or <strong>Reject all</strong>. See Imber › Reviewing changes.
</p>

<h2>See the request before it is sent</h2>
<p>
  The <strong>…</strong> menu has <em>Show what would be sent…</em>, which opens the exact request for the next message without sending it, and <em>Dry run</em>, which makes Send show the request instead of making it. Both list where the size comes from and a rough cost. The key is masked.
</p>

<h2>Project instructions are read automatically</h2>
<p>
  The project's <code>CLAUDE.md</code>, <code>.claude/CLAUDE.md</code> and <code>AGENTS.md</code>, and your own <code>assistant-instructions.md</code> in Bennu's config folder, are added to Imber's instructions, so it follows the build command, conventions and no-go areas a team has written down. Each file is cut at 16 KB and all together at 40 KB, and a file whose text repeats another is sent once. They appear as chips above the text. They never override the access prompts or the ban on a shell. The switch is Settings › Imber › New conversations › Project instructions.
</p>
