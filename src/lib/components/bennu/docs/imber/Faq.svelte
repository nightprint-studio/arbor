<span class="eyebrow">Imber</span>
<h1>Imber: Troubleshooting</h1>

<p class="doc-lead">
  The messages Imber and Bennu show most often, what each one means, and what to do.
</p>

<h2>"No API key is set. Add one in Settings › Imber."</h2>
<p>Open Settings › Imber › Connection, paste your Anthropic key and press Save. See Imber › Getting started.</p>

<h2>"Open a project first — Imber works inside one"</h2>
<p>Agent conversations need an open project. Open one, or switch to Chat for a question that has nothing to do with a project.</p>

<h2>A run stopped with a limit message ("Raise the limit in Settings › Imber › Spending limits to go on")</h2>
<p>A conversation, day, week or month reached its limit in euros. Raise the limit under Settings › Imber › Spending limits, or set it to 0 to switch it off. See Imber › Cost and context.</p>

<h2>"… is outside what you allowed. Open it for reading, or for reading and writing?"</h2>
<p>Imber reached for a file outside the reach. Choose Deny, Allow reading or Allow reading and writing; the answer covers that folder for the session. To avoid the question, widen the reach or add the folder under Settings › Imber › What it may reach. See Imber › Permissions and modes.</p>

<h2>"The user has switched that kind of tool off" / "this is a chat"</h2>
<p>The family is off in Settings › Imber › Tools it may use, or the conversation is a Chat. Turn the family on, or start an Agent conversation.</p>

<h2>"Editing is switched off"</h2>
<p>Edit files is off (Settings › Imber, or composer › Settings › Permissions). Imber is read-only until you turn it on.</p>

<h2>"… holds secrets and is never written" / "the repository's own .git files are never changed here"</h2>
<p>Hard limits. Edit the file yourself.</p>

<h2>The status line says "No connection - retrying" (or "The API is overloaded", "Rate limit reached")</h2>
<p>A request failed for a reason that usually passes, and Imber is waiting to send it again. The line shows the attempt and the seconds left; <strong>Retry now</strong> skips the wait and <strong>Stop</strong> ends the run. When the connection comes back the line disappears and the answer continues; nothing you did in between is lost. See Imber › Cost and context › When a request fails.</p>

<h2>A run ended with "You are offline", "The API stopped answering" or "The connection dropped"</h2>
<p>Imber waited for the connection for about three minutes and it did not come back. Everything that had finished is saved. Fix the connection (or the VPN/proxy), then press <strong>Retry</strong> under the message: the same request is sent again, with no new message from you.</p>

<h2>"The API key was rejected"</h2>
<p>The key is invalid, revoked, or may not use that model. Open Settings › Imber › Connection, save a valid key, then press <strong>Retry</strong>. Nothing is retried automatically: waiting would not help.</p>

<h2>"The account has no credit left or reached a usage limit"</h2>
<p>The provider refused because of the account, not Imber's own spending limits. Check the plan and billing of the provider account, then press <strong>Retry</strong>.</p>

<h2>"The conversation no longer fits the model's window"</h2>
<p>Compact the conversation (the panel's <strong>…</strong> menu › Compact), or start a new one, then press <strong>Retry</strong>. See Imber › Cost and context.</p>

<h2>"The rate limit was reached" / "The API is overloaded"</h2>
<p>Imber already tried five times, waiting at least as long as the provider asked. Wait a minute and press <strong>Retry</strong>, or use a lighter model or effort.</p>

<h2>A sub-agent failed with a connection error</h2>
<p>The agent's card says what failed; the conversation that started it is told the same, including whether the agent had already changed files. Ask for the agent again; its earlier changes stay.</p>

<h2>The proposal card says Outdated</h2>
<p>A file changed after Imber read it. Nothing was written. Use Re-apply on current file, Ask to redo, or Discard. See Imber › Reviewing changes.</p>

<h2>"Wait for Imber to finish, or stop it first"</h2>
<p>The action (deleting a conversation, applying or discarding an isolated copy) needs the run to be over. Press Stop, or wait.</p>

<h2>Another conversation is changing this file</h2>
<p>A file is reserved by the conversation that is changing it until its run ends. Wait for that run, work on another file, or stop the other conversation. Reading is never blocked.</p>

<h2>A build waits behind another</h2>
<p>Builds from every conversation and the Build button share one queue per project. Open the Live panel to see it. For Cargo projects, composer › Build isolation › Separate target avoids the wait at the cost of disk space.</p>

<h2>"A git worktree needs git access, which is off"</h2>
<p>Turn on Settings › Imber › Git, or use the text-only copy.</p>

<h2>"The project is too large for a plain copy"</h2>
<p>A plain isolated copy is capped at 512 MB or 40,000 files. In a git repository use the worktree kind (git access must be on).</p>

<h2>The conversation got expensive</h2>
<p>Check the context meter's breakdown. Compact the conversation, start a new one, lower the effort, switch off families you do not need (screenshots are costly), or use a cheaper model. See Imber › Cost and context.</p>

<h2>Answers ignore a project rule</h2>
<p>Check that Settings › Imber › Project instructions is on and that the file is among the chips above the composer. Files are cut at 16 KB each and 40 KB together.</p>

<h2>A skill is not used</h2>
<p>A skill is invisible until enabled in the skills manager. Check Imber › Skills for the folders, the format, and the size limits.</p>

<h2>"Imber could not write a diagram that parses"</h2>
<p>After three tries the source is shown as text. Copy it from the notice, or ask Imber to simplify the diagram.</p>

<h2>A debug session ended by itself</h2>
<p>An unused session ends after the idle time in Settings › Imber (30 minutes by default), when the program exits, or when the project closes. See Imber › Debugging with Imber.</p>

<h2>Imber says it cannot do something Bennu can</h2>
<p>It acts only through its tools: no shell, no push, nothing outside the reach. Check Imber › Tools it can use for the tool, and the family switch.</p>
