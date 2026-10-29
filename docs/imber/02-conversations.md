---
title: "Imber: Conversations"
summary: "Saved conversations, rename and delete, Agent versus Chat, the home screen, the step plan."
keywords: ["history", "rename", "delete", "chat only", "agent", "home", "thread", "plan", "document"]
related: ["Imber: Getting started", "Imber: Reviewing changes", "Imber: Tools it can use"]
source: src/lib/components/bennu/docs/imber/Conversations.svelte
---
<!-- Generated from the in-app docs - do not edit. Edit the Svelte pages and run `node scripts/build-help-index.mjs`. -->

# Imber: Conversations

Every conversation with Imber is saved on disk, listed by the title in the panel header, and is either an agent conversation (works in the project) or a chat (talks only).

## The conversation switcher

The title in the header is a switcher. It lists your conversations, today's first, each with the model it used, when it was last touched and what it cost. The open one shows as saved. Choosing a row opens that conversation, with its thread and its context meter.

- **New conversation**: the pencil-and-square button in the header, or the command *Imber: New conversation in an isolated copy* for a conversation in a copy.
- **Rename**: the pencil next to the title.
- **Delete**: the trash next to the title, or *Delete conversation* in the **…** menu. It asks first. Deleting a conversation also removes its isolated copy, its index and the processes started in it. The money it cost stays counted against your limits: costs are kept in a ledger of their own.

Conversations are plain files in Bennu's data folder (`arbor/data/bennu/assistant`), one per conversation, holding both what the panel draws and what is sent to the model next time.

## Agent or Chat

The **Agent / Chat** switch in the header chooses between two kinds of conversation. Each has its own list and its own home screen, and a conversation is one or the other for good; opening a chat from the list switches the panel to Chat.

|   | Agent | Chat |
| --- | --- | --- |
| **Works in the project** | Yes: reads, searches, edits, builds, runs, debugs through Bennu's tools | No. It cannot read, search or change anything |
| **What it sees** | The project, the open file, the selection, the instruction files | Only what you type or select |
| **Tools** | All the families you left on | Three: the help about Bennu, opening a panel, drawing a diagram |
| **Modes and reach** | Mode menu and Reach apply | Not shown: nothing to ask |
| **Good for** | Anything about the project | A question unrelated to the project, or when you want to be sure nothing is touched |

**Chat only** in Settings › Imber › New conversations makes new conversations chats by default. Git, skills and sub-agents are not available in a chat.

## The home screen

With no conversation open the panel shows its home. An agent home offers six starters about the project (explain this file, explain the project, show me a panel, find why it fails, document it, review my changes); a chat home offers four that need no project (explain a concept, write a regex, rewrite a text, think it through). It also lists your four most recent conversations and the month's spending so far.

## What is in the thread

- Your messages on the right; Imber's as flowing Markdown (headings, lists, tables, quotes, highlighted code) that you can select. Hover an answer to copy it or **open it as a document**, a Markdown editor where you can edit and save it without changing the conversation. Hover your own message to copy it.
- Tools it used, folded into one line that opens on demand ("Read 2 files · Searched once").
- Cards for proposals ([Imber › Reviewing changes](04-reviewing-changes.md)), questions, access prompts, the step plan, sub-agents, live runs and debugger stops.
- Notices in the thread: a compaction, a limit reached, an access grant, a system note about another conversation.

## The step plan

For work with several steps Imber writes a plan first and ticks the steps off. The card lists them (done, in progress, still to do) with a bar and "3 of 7 · 43%". The percentage is steps done over steps planned: it says how much of the list is done, not how long the rest will take, and it goes down if the plan grows. The plan costs a few hundred output tokens per update, a few cents for a task of several steps. Questions and small changes get no plan. **Step plan** in the composer's settings menu (and in Settings › Imber) switches it off; the tool and its instruction are then not sent at all.

## Several conversations at once

Two conversations can run on the same project. A file one of them is changing is reserved to it until its run ends: the other is refused, naming the conversation, and can wait or work elsewhere (reading is never blocked). Services and debug sessions belong to the conversation that started them. Builds queue. See [Imber › Running and observing](06-running-and-observing.md).
