---
title: "Imber: Troubleshooting"
summary: "Common messages and what to do about them."
keywords: ["error", "message", "troubleshooting", "faq", "fix", "limit reached", "outdated", "no api key", "no connection", "offline", "retry", "rate limit", "overloaded", "api key rejected", "timeout"]
related: ["Imber: Getting started", "Imber: Permissions and modes", "Imber: Cost and context"]
source: src/lib/components/bennu/docs/imber/Faq.svelte
---
<!-- Generated from the in-app docs - do not edit. Edit the Svelte pages and run `node scripts/build-help-index.mjs`. -->

# Imber: Troubleshooting

The messages Imber and Bennu show most often, what each one means, and what to do.

## "No API key is set. Add one in Settings › Imber."

Open Settings › Imber › Connection, paste your Anthropic key and press Save. See [Imber › Getting started](01-getting-started.md).

## "Open a project first — Imber works inside one"

Agent conversations need an open project. Open one, or switch to Chat for a question that has nothing to do with a project.

## A run stopped with a limit message ("Raise the limit in Settings › Imber › Spending limits to go on")

A conversation, day, week or month reached its limit in euros. Raise the limit under Settings › Imber › Spending limits, or set it to 0 to switch it off. See [Imber › Cost and context](13-cost-and-context.md).

## "… is outside what you allowed. Open it for reading, or for reading and writing?"

Imber reached for a file outside the reach. Choose Deny, Allow reading or Allow reading and writing; the answer covers that folder for the session. To avoid the question, widen the reach or add the folder under Settings › Imber › What it may reach. See [Imber › Permissions and modes](03-permissions.md).

## "The user has switched that kind of tool off" / "this is a chat"

The family is off in Settings › Imber › Tools it may use, or the conversation is a Chat. Turn the family on, or start an Agent conversation.

## "Editing is switched off"

Edit files is off (Settings › Imber, or composer › Settings › Permissions). Imber is read-only until you turn it on.

## "… holds secrets and is never written" / "the repository's own .git files are never changed here"

Hard limits. Edit the file yourself.

## The status line says "No connection - retrying" (or "The API is overloaded", "Rate limit reached")

A request failed for a reason that usually passes, and Imber is waiting to send it again. The line shows the attempt and the seconds left; **Retry now** skips the wait and **Stop** ends the run. When the connection comes back the line disappears and the answer continues; nothing you did in between is lost. See [Imber › Cost and context](13-cost-and-context.md) › When a request fails.

## A run ended with "You are offline", "The API stopped answering" or "The connection dropped"

Imber waited for the connection for about three minutes and it did not come back. Everything that had finished is saved. Fix the connection (or the VPN/proxy), then press **Retry** under the message: the same request is sent again, with no new message from you.

## "The API key was rejected"

The key is invalid, revoked, or may not use that model. Open Settings › Imber › Connection, save a valid key, then press **Retry**. Nothing is retried automatically: waiting would not help.

## "The account has no credit left or reached a usage limit"

The provider refused because of the account, not Imber's own spending limits. Check the plan and billing of the provider account, then press **Retry**.

## "The conversation no longer fits the model's window"

Compact the conversation (the panel's **…** menu › Compact), or start a new one, then press **Retry**. See [Imber › Cost and context](13-cost-and-context.md).

## "The rate limit was reached" / "The API is overloaded"

Imber already tried five times, waiting at least as long as the provider asked. Wait a minute and press **Retry**, or use a lighter model or effort.

## A sub-agent failed with a connection error

The agent's card says what failed; the conversation that started it is told the same, including whether the agent had already changed files. Ask for the agent again; its earlier changes stay.

## The proposal card says Outdated

A file changed after Imber read it. Nothing was written. Use Re-apply on current file, Ask to redo, or Discard. See [Imber › Reviewing changes](04-reviewing-changes.md).

## "Wait for Imber to finish, or stop it first"

The action (deleting a conversation, applying or discarding an isolated copy) needs the run to be over. Press Stop, or wait.

## Another conversation is changing this file

A file is reserved by the conversation that is changing it until its run ends. Wait for that run, work on another file, or stop the other conversation. Reading is never blocked.

## A build waits behind another

Builds from every conversation and the Build button share one queue per project. Open the Live panel to see it. For Cargo projects, composer › Build isolation › Separate target avoids the wait at the cost of disk space.

## "A git worktree needs git access, which is off"

Turn on Settings › Imber › Git, or use the text-only copy.

## "The project is too large for a plain copy"

A plain isolated copy is capped at 512 MB or 40,000 files. In a git repository use the worktree kind (git access must be on).

## The conversation got expensive

Check the context meter's breakdown. Compact the conversation, start a new one, lower the effort, switch off families you do not need (screenshots are costly), or use a cheaper model. See [Imber › Cost and context](13-cost-and-context.md).

## Answers ignore a project rule

Check that Settings › Imber › Project instructions is on and that the file is among the chips above the composer. Files are cut at 16 KB each and 40 KB together.

## A skill is not used

A skill is invisible until enabled in the skills manager. Check [Imber › Skills](10-skills.md) for the folders, the format, and the size limits.

## "Imber could not write a diagram that parses"

After three tries the source is shown as text. Copy it from the notice, or ask Imber to simplify the diagram.

## A debug session ended by itself

An unused session ends after the idle time in Settings › Imber (30 minutes by default), when the program exits, or when the project closes. See [Imber › Debugging with Imber](07-debugging.md).

## Imber says it cannot do something Bennu can

It acts only through its tools: no shell, no push, nothing outside the reach. Check [Imber › Tools it can use](05-tools.md) for the tool, and the family switch.
