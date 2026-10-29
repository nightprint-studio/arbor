---
title: "Imber: Sub-agents"
summary: "Clean-context helpers for side tasks: when to use them, permissions, cost cap, how to watch and stop them."
keywords: ["sub-agent", "subagent", "helper", "spawn_agent", "parallel", "side model"]
related: ["Imber: Cost and context", "Imber: Permissions and modes", "Imber: Settings reference"]
source: src/lib/components/bennu/docs/imber/Subagents.svelte
---
<!-- Generated from the in-app docs - do not edit. Edit the Svelte pages and run `node scripts/build-help-index.mjs`. -->

# Imber: Sub-agents

A sub-agent is a helper Imber starts with a clean context: it knows only its task, works with a narrower set of tools, and sends back only a summary, so what it reads never fills the conversation's own window.

## When to use them

They are for independent side tasks, for exploration that would read many files to answer one question, and for work unrelated to what the conversation was doing. They are not for anything a call or two answers.

## Turning them on

Off by default. **Settings › Imber › New conversations › Sub-agents** turns them on for every conversation. In the composer's settings menu, *Sub-agents* (or the palette entry *Imber: Sub-agents in this conversation — next*) cycles through *Follow the settings*, *On* and *Off* for one conversation, applying from its next message. When they are off Imber is not even told they exist. Not available in a chat.

## What an agent can do

- **Read-only by default.** An agent reads and searches on the cheaper side model (Settings › Imber › Cheaper model for side jobs), and up to three run at once. Imber can go on while they work and collects their summaries later (`spawn_agent`, `await_agents`).
- **Editing** is possible only when the conversation's own edits are applied rather than proposed (Accept edits or Autonomous, with editing on). Then Imber waits for the agent, so only one of them writes at a time, and file reservations apply as for any conversation.
- **Limits per agent:** 25 steps, one level of agents (an agent cannot start agents), a summary of at most 6,000 characters.

## Never more than Imber

An agent's access is the conversation's, narrowed. It cannot reach anything Imber could not, never deletes without asking, does not inherit files opened for Imber's own run, and under Autonomous it runs as Accept edits: it does not open files by itself. Anything beyond what it has is asked of you, in the conversation's thread, labelled with the agent's name; what you allow it is the agent's alone. It cannot ask you questions, start agents or write to other conversations.

## Cost

What an agent spends is added to the conversation and counts against the spending limits like Imber's own requests. Each agent also has a cap of its own, a third of the session limit and at most €1, and stops when it reaches it. Stopping the conversation's run stops its agents.

## Seeing them

- Above the thread an *Agents* list shows each agent's state, how long it has run and what it cost.
- The title opens its thread, read-only and live (*Imber: Open the latest sub-agent's thread…*). It reads as a chat between the two: the task Imber (badged *main*) gave it, then the agent's own messages (badged *sub-agent*), a writing mark while it works and, at the end, a line saying it handed its summary back, stopped or failed. The view follows the end of the thread unless you scrolled up.
- Each agent has a Stop; *Stop all* and *Imber: Stop all sub-agents* stop them together.
- A card for each stays in the thread: what it was asked, what it may do, its state, time and cost, and the start of its answer. While the agent runs the card's border moves. An agent's record and thread are kept on disk for as long as its conversation keeps them, so the Agents list and an agent's thread still open after a restart; an agent that was running when the app closed shows as stopped. What it read is not copied: the kept thread says *read this file*, not the file. Imber can continue a finished agent with `resume_agent` and a new instruction: it remembers what it concluded and reads again what it needs. The trash button on a finished agent deletes it; compacting the conversation, or deleting it, deletes them all.

## A hint, not a decision

When the window is filling up (from 70% at medium effort, 50% at high; never at low) and your new message looks unrelated to the files worked on so far, a one-line note is added at the end of that message suggesting an agent. Imber decides.
