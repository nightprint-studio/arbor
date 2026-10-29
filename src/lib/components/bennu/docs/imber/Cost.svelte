<span class="eyebrow">Imber</span>
<h1>Imber: Cost and context</h1>

<p class="doc-lead">
  Every request carries the whole conversation, so the cost depends less on how much Imber writes than on how much it re-reads; Bennu keeps that small, shows where tokens go, and stops a run at the limits you set.
</p>

<h2>The context meter</h2>
<p>
  Above the composer a bar shows how full the model's window is, split into <em>tool definitions</em>, <em>Imber rules</em>, <em>project instructions</em>, the <em>skills index</em>, <em>files and search results</em>, and the <em>conversation</em>. It stays calm until 75%, turns amber there and red past 90%. Open the line for the breakdown and for how much of the last request came from the provider's cache. Every figure is an estimate (about four characters to a token); the exact count comes back with each answer.
</p>
<p>
  <em>Show what would be sent…</em> (the panel's <strong>…</strong> menu) lists the same parts for the next request, in the order the provider reads them, with the cost when the cache is cold and when it is warm.
</p>

<h2>Models, prices and the context window</h2>
<p>
  The model list and prices are a file, <code>models.toml</code>, built in. <strong>Settings › Imber › Models and prices › Create a price file</strong> creates your own copy (<code>assistant-models.toml</code> in Bennu's config folder): an entry with the same id replaces the built-in one and any other id is added, so a price change or a new model needs an edit and a <em>Reload</em>, not a new build. An entry gives <code>[model.price]</code> (<code>input</code>, <code>output</code> and optionally <code>cache_read</code>, <code>cache_write_5m</code>, <code>cache_write_1h</code>), the <code>context_max</code> window, the <code>efforts</code> it accepts and, for a model that charges more for long prompts, <code>[model.long_prompt]</code> with <code>over_tokens</code> and its own rates. Cost is worked out request by request, with cache reads and writes at their own rates.
</p>
<p>
  The <strong>context window</strong> is the largest window Imber may use in a conversation, 50K up to 1M; a size the model cannot hold is listed but cannot be picked, and changing to a smaller model shrinks the window to fit.
</p>

<h2>How the cost is kept down</h2>
<ul>
  <li><strong>The provider's cache.</strong> Tool definitions, instructions and the conversation so far are cached, so a request pays full price only for what the previous one did not already send. Anything that changes the start of a request starts it over, which is why the open file travels with your message and not with the instructions.</li>
  <li><strong>Long cache.</strong> By default the tools and instructions are kept in the one-hour cache (Settings › Imber › Keep instructions cached for an hour) instead of the five-minute one. It costs a little more the first time and pays off when you pause for more than five minutes between messages or open another conversation in the same project.</li>
  <li><strong>Reading the same thing twice.</strong> A file already read comes back as one line when unchanged, and as the lines that differ when changed. <code>file_outline</code> and <code>read_files</code> help open big files by the part that matters.</li>
  <li><strong>Big results are digests.</strong> Counts, grouped problems, the files most affected, and a statement of what was left out. <code>expand_result</code> gives the detail.</li>
  <li><strong>Old results are stubbed in a batch.</strong> When many old file reads and search results have piled up they are replaced all at once by one-line stubs saying how to fetch them again, and the thread says so. In a batch, never one per message, because changing a message makes the cache start over from it.</li>
</ul>

<h2>Compacting</h2>
<p>
  <strong>Compact conversation…</strong> (offered by the context meter) first clears the output of older tool calls, then folds everything before your last message into a summary. What you type in the dialog is what the summary must not lose, as with <code>/compact "…"</code> in Claude Code.
</p>
<ul>
  <li>The summary is written by the <strong>cheaper side model</strong> (Settings › Imber › Cheaper model for side jobs; by default the cheapest Haiku in the price list), with fixed sections: goal, decisions, state, open questions and your preferences, followed by the files changed and the current plan taken from the record word for word.</li>
  <li>It happens when you ask, automatically when the conversation reaches 90% of the largest window you chose (clearing first, summarising only if that is not enough), and at the end of a run once the conversation is past 60% of the window.</li>
  <li>The thread says how many messages became how many tokens and which tool output was not kept. Compacting costs one request, added to the conversation's cost.</li>
</ul>

<h2>Changing model or window in a long conversation</h2>
<p>
  A new model cannot reuse what the old one has cached, so a conversation of several thousand tokens sends all of it again at full price. Bennu asks first, with an estimate. A smaller context window asks only when the conversation already fills it (it would be compacted first); a larger one changes nothing. Turn the question off with <strong>Settings › Imber › Confirm costly changes</strong>.
</p>

<h2>When a request fails</h2>
<p>
  A failed request is told apart by what caused it, and only the causes that pass by themselves are tried again.
</p>
<ul>
  <li><strong>Sent again, automatically:</strong> no connection or DNS failure, a connection that goes silent (no data for two minutes) or drops while the answer streams, rate limit (429), overloaded (529) and server errors (5xx).</li>
  <li><strong>Never retried:</strong> a rejected key (401/403), no credit or an account usage limit, a conversation too long for the window, an unknown model, an invalid request, and your own Stop. These end the run at once with the reason and what to do.</li>
  <li><strong>How long it waits:</strong> 1, 2, 4, 8, 16 and then 30 seconds, each spread a little at random so agents that fail together do not return together, never less than the provider's own <code>retry-after</code>. A network that stays down is waited out for about three minutes in total; a provider that keeps answering "not now" is asked five times. A <code>retry-after</code> longer than two minutes is reported rather than waited for.</li>
  <li><strong>While it waits</strong> the thread shows one calm line with the attempt and the seconds left: <strong>Retry now</strong> skips the wait, <strong>Stop</strong> ends the run at once. There is no toast per attempt. When the connection returns the line goes and the answer continues; the text the broken attempt had already shown is taken back and written again.</li>
  <li><strong>Money:</strong> an attempt that failed after the provider had started it can still have been billed; it is counted once, as a request of its own. Spending limits are checked before every retry exactly as before a first request.</li>
  <li><strong>When it gives up</strong> the message says what happened and the next step; the provider's own words are under <em>Details</em>. Every turn that finished is saved. <strong>Retry</strong> under the message sends the same request again, with no new message from you.</li>
  <li><strong>Compaction</strong> (manual, automatic and at the end of a run) waits only about twenty seconds, then stops with the reason; the conversation is left exactly as it was and the next run tries again.</li>
  <li><strong>Sub-agents</strong> retry the same way; their card shows <em>waiting to retry</em>. A failed agent reports to the conversation what failed, whether starting it again can help, and whether it had already changed files.</li>
</ul>

<h2>Spending limits</h2>
<p>
  Settings › Imber › Spending limits holds limits in euros: <strong>per conversation</strong> (default €5), <strong>per day</strong> (from midnight, your time; €10), <strong>per week</strong> (from Monday; off), <strong>per month</strong> (from the first; €50). <code>0</code> switches one off.
</p>
<ul>
  <li>A conversation or a period at its limit does not start another request, and a run in progress stops before the request that would take it over; the thread says which limit.</li>
  <li>The composer flags a limit from 80% of it (<strong>Warn from</strong>, adjustable) and shows what the conversation has cost.</li>
  <li>Limits count what conversations still running have spent so far, so two at once cannot together pass a daily, weekly or monthly limit that neither passes alone.</li>
  <li>Costs are recorded per run in a ledger of their own (<code>usage.jsonl</code> in the data folder): deleting a conversation does not hand its money back.</li>
  <li>Prices are in dollars, so amounts are converted at the <strong>Exchange rate</strong> you keep by hand (default 0.92 euros per dollar); with no usable rate a dollar counts as a euro. The rate never moves on its own.</li>
  <li>Sub-agents count against the same limits and have a cap of their own (Imber › Sub-agents).</li>
</ul>
