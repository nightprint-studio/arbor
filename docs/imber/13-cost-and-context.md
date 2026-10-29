---
title: "Imber: Cost and context"
summary: "Context meter, models and prices, cache, compaction, confirm-on-change and spending limits in euros."
keywords: ["cost", "tokens", "cache", "long cache", "compact", "context window", "limit", "euro", "price", "side model", "models.toml", "retry", "timeout", "offline", "rate limit", "overloaded"]
related: ["Imber: Settings reference", "Imber: Sub-agents", "Imber: Getting started"]
source: src/lib/components/bennu/docs/imber/Cost.svelte
---
<!-- Generated from the in-app docs - do not edit. Edit the Svelte pages and run `node scripts/build-help-index.mjs`. -->

# Imber: Cost and context

Every request carries the whole conversation, so the cost depends less on how much Imber writes than on how much it re-reads; Bennu keeps that small, shows where tokens go, and stops a run at the limits you set.

## The context meter

Above the composer a bar shows how full the model's window is, split into *tool definitions*, *Imber rules*, *project instructions*, the *skills index*, *files and search results*, and the *conversation*. It stays calm until 75%, turns amber there and red past 90%. Open the line for the breakdown and for how much of the last request came from the provider's cache. Every figure is an estimate (about four characters to a token); the exact count comes back with each answer.

*Show what would be sent…* (the panel's **…** menu) lists the same parts for the next request, in the order the provider reads them, with the cost when the cache is cold and when it is warm.

## Models, prices and the context window

The model list and prices are a file, `models.toml`, built in. **Settings › Imber › Models and prices › Create a price file** creates your own copy (`assistant-models.toml` in Bennu's config folder): an entry with the same id replaces the built-in one and any other id is added, so a price change or a new model needs an edit and a *Reload*, not a new build. An entry gives `[model.price]` (`input`, `output` and optionally `cache_read`, `cache_write_5m`, `cache_write_1h`), the `context_max` window, the `efforts` it accepts and, for a model that charges more for long prompts, `[model.long_prompt]` with `over_tokens` and its own rates. Cost is worked out request by request, with cache reads and writes at their own rates.

The **context window** is the largest window Imber may use in a conversation, 50K up to 1M; a size the model cannot hold is listed but cannot be picked, and changing to a smaller model shrinks the window to fit.

## How the cost is kept down

- **The provider's cache.** Tool definitions, instructions and the conversation so far are cached, so a request pays full price only for what the previous one did not already send. Anything that changes the start of a request starts it over, which is why the open file travels with your message and not with the instructions.
- **Long cache.** By default the tools and instructions are kept in the one-hour cache (Settings › Imber › Keep instructions cached for an hour) instead of the five-minute one. It costs a little more the first time and pays off when you pause for more than five minutes between messages or open another conversation in the same project.
- **Reading the same thing twice.** A file already read comes back as one line when unchanged, and as the lines that differ when changed. `file_outline` and `read_files` help open big files by the part that matters.
- **Big results are digests.** Counts, grouped problems, the files most affected, and a statement of what was left out. `expand_result` gives the detail.
- **Old results are stubbed in a batch.** When many old file reads and search results have piled up they are replaced all at once by one-line stubs saying how to fetch them again, and the thread says so. In a batch, never one per message, because changing a message makes the cache start over from it.

## Compacting

**Compact conversation…** (offered by the context meter) first clears the output of older tool calls, then folds everything before your last message into a summary. What you type in the dialog is what the summary must not lose, as with `/compact "…"` in Claude Code.

- The summary is written by the **cheaper side model** (Settings › Imber › Cheaper model for side jobs; by default the cheapest Haiku in the price list), with fixed sections: goal, decisions, state, open questions and your preferences, followed by the files changed and the current plan taken from the record word for word.
- It happens when you ask, automatically when the conversation reaches 90% of the largest window you chose (clearing first, summarising only if that is not enough), and at the end of a run once the conversation is past 60% of the window.
- The thread says how many messages became how many tokens and which tool output was not kept. Compacting costs one request, added to the conversation's cost.

## Changing model or window in a long conversation

A new model cannot reuse what the old one has cached, so a conversation of several thousand tokens sends all of it again at full price. Bennu asks first, with an estimate. A smaller context window asks only when the conversation already fills it (it would be compacted first); a larger one changes nothing. Turn the question off with **Settings › Imber › Confirm costly changes**.

## When a request fails

A failed request is told apart by what caused it, and only the causes that pass by themselves are tried again.

- **Sent again, automatically:** no connection or DNS failure, a connection that goes silent (no data for two minutes) or drops while the answer streams, rate limit (429), overloaded (529) and server errors (5xx).
- **Never retried:** a rejected key (401/403), no credit or an account usage limit, a conversation too long for the window, an unknown model, an invalid request, and your own Stop. These end the run at once with the reason and what to do.
- **How long it waits:** 1, 2, 4, 8, 16 and then 30 seconds, each spread a little at random so agents that fail together do not return together, never less than the provider's own `retry-after`. A network that stays down is waited out for about three minutes in total; a provider that keeps answering "not now" is asked five times. A `retry-after` longer than two minutes is reported rather than waited for.
- **While it waits** the thread shows one calm line with the attempt and the seconds left: **Retry now** skips the wait, **Stop** ends the run at once. There is no toast per attempt. When the connection returns the line goes and the answer continues; the text the broken attempt had already shown is taken back and written again.
- **Money:** an attempt that failed after the provider had started it can still have been billed; it is counted once, as a request of its own. Spending limits are checked before every retry exactly as before a first request.
- **When it gives up** the message says what happened and the next step; the provider's own words are under *Details*. Every turn that finished is saved. **Retry** under the message sends the same request again, with no new message from you.
- **Compaction** (manual, automatic and at the end of a run) waits only about twenty seconds, then stops with the reason; the conversation is left exactly as it was and the next run tries again.
- **Sub-agents** retry the same way; their card shows *waiting to retry*. A failed agent reports to the conversation what failed, whether starting it again can help, and whether it had already changed files.

## Spending limits

Settings › Imber › Spending limits holds limits in euros: **per conversation** (default €5), **per day** (from midnight, your time; €10), **per week** (from Monday; off), **per month** (from the first; €50). `0` switches one off.

- A conversation or a period at its limit does not start another request, and a run in progress stops before the request that would take it over; the thread says which limit.
- The composer flags a limit from 80% of it (**Warn from**, adjustable) and shows what the conversation has cost.
- Limits count what conversations still running have spent so far, so two at once cannot together pass a daily, weekly or monthly limit that neither passes alone.
- Costs are recorded per run in a ledger of their own (`usage.jsonl` in the data folder): deleting a conversation does not hand its money back.
- Prices are in dollars, so amounts are converted at the **Exchange rate** you keep by hand (default 0.92 euros per dollar); with no usable rate a dollar counts as a euro. The rate never moves on its own.
- Sub-agents count against the same limits and have a cap of their own ([Imber › Sub-agents](11-sub-agents.md)).
