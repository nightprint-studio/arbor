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
- **Title**: in its first reply Imber names the conversation after the task (a few words, in your language), and the tab follows at once. Afterwards it changes the title only when you ask it to; a title you set yourself stays yours.
- **Rename**: the pencil next to the title.
- **Delete**: the trash next to the title, or *Delete conversation* in the **…** menu. It asks first. Deleting a conversation also removes its isolated copy, its index and the processes started in it. The money it cost stays counted against your limits: costs are kept in a ledger of their own.

Conversations are plain files in Bennu's data folder (`arbor/data/bennu/assistant`), one per conversation, holding both what the panel draws and what is sent to the model next time.

## Agent or Chat

The **Agent / Chat** switch in the header chooses between two kinds of conversation. Each has its own list and its own home screen, and a conversation is one or the other for good; opening a chat from the list switches the panel to Chat.

|   | Agent | Chat |
| --- | --- | --- |
| **Works in the project** | Yes: reads, searches, edits, builds, runs, debugs through Bennu's tools | No. It cannot read, search or change anything |
| **What it sees** | The project, the open file, the selection, the instruction files | Only what you type or select |
| **Tools** | All the families you left on | Four: the help about Bennu, opening a panel, drawing a diagram, showing a picture |
| **Modes and reach** | Mode menu and Reach apply | Not shown: nothing to ask |
| **Good for** | Anything about the project | A question unrelated to the project, or when you want to be sure nothing is touched |

**Chat only** in Settings › Imber › New conversations makes new conversations chats by default. Git, skills and sub-agents are not available in a chat.

## The home screen

With no conversation open the panel shows its home. An agent home offers six starters about the project (explain this file, explain the project, show me a panel, find why it fails, document it, review my changes); a chat home offers four that need no project (explain a concept, write a regex, rewrite a text, think it through). It also lists your four most recent conversations and the month's spending so far.

## What is in the thread

The thread follows new text while you are at its end; scroll up and it stays where you are, scroll back to the end and it follows again. Each conversation reopens where you left it — closing the panel or switching conversation and coming back lands at the same place, or at the end if that is where you were. Expanded over the editor (`Ctrl+Shift+U`) the thread is a wide column and the conversation and its context meter share one row.

- Your messages on the right; Imber's as flowing Markdown (headings, lists, tables, quotes, highlighted code) that you can select. Hover an answer to copy it or **open it as a document**, a Markdown editor where you can edit and save it without changing the conversation. Hover your own message to copy it.
- Tools it used, folded into one line that opens on demand ("Read 2 files · Searched once · 41s"). Inside, each call says how long it ran, always at the right edge (and, in amber, how long it waited for your answer on a card); its icon turns red when it failed or was refused, and hovering it shows the whole of what came back, and a *Model answered* row says how long each answer of the model took and how many tokens it wrote; hover it to see how much of that was waiting for the first token.
- Under a finished answer, where its time went: the total, then *model*, *tools* and *waiting for you* apart — the way to tell a slow tool from a model that writes a lot.
- Cards for proposals ([Imber › Reviewing changes](04-reviewing-changes.md)), questions, access prompts, the step plan, sub-agents, live runs and debugger stops.
- Notices in the thread: a compaction, a limit reached, an access grant, a system note about another conversation.

## The step plan

For work with several steps Imber writes a plan first and ticks the steps off. The card lists them (done, in progress, still to do) with a bar and "3 of 7 · 43%". The percentage is steps done over steps planned: it says how much of the list is done, not how long the rest will take, and it goes down if the plan grows. The plan costs a few hundred output tokens per update, a few cents for a task of several steps. Questions and small changes get no plan. **Step plan** in the composer's settings menu (and in Settings › Imber) switches it off; the tool and its instruction are then not sent at all. The plan is also part of what Imber reads back: after a stop, a *continue* or a compaction it still has the latest plan, word for word, and a new message in a conversation whose plan is still open reminds it where the plan got to. While it works, a plan left untouched for five tool calls is recalled to it with the step in progress, so steps are ticked as they are done; and if it ends its answer with steps still open, it is asked once to tick them or to say what is left.

## The strip on the message box

A strip on top of the message box says what the conversation has going on, so you never scroll back to find a card. **Plan** shows the steps done (*3/6*) and names the step being worked on, with the plan's progress as a line along the strip's foot; **Agents** says how many are running, and **Review** how many proposals wait for your decision. Each segment is there only when it has something to show. Press one to open its panel over the thread — every step of the plan, with the one in progress brought to the middle and marked *Now*; every sub-agent with its Stop; every proposal with *Review…*. Press it again, Esc or a click outside closes it. The command palette opens the same panels (*Imber: Show the plan*, *Show the sub-agents*, *Show the edits to review*).

## Writing while it works

You do not have to wait or stop Imber to say something. While it works, write in the message box and press `Ctrl+Enter` (or the arrow beside Stop): the message waits above the box, marked *Waiting*, and Imber reads it before its next request — after the tool it is running, without stopping. In the thread it then stands where it was read, as *You, while it worked*. Words only: files go with your next ordinary message. If the run ends before reading it, the message comes back into the box for you to send.

## Steps and non-stop

A run makes at most **Steps per run** requests to the model (Settings › [Imber › Conversations](02-conversations.md), 40 by default; a step is one request, however many tools it calls). When it gets there it stops with “Stopped after N steps” and waits for you to say *continue*, which picks up where it was.

**Non-stop**, in the composer's settings menu or the command palette (*Imber: Non-stop in this conversation*), lifts that cap for one conversation: its runs go on until the work is done or you stop them. The spending limits still stop it, and a scheduled run in that conversation keeps the cap. The choice is kept with the conversation. While it is on, the settings button carries a small ∞ and the menu's row reads *On*.

## Several conversations at once

Several conversations can work at the same time: start one, open another and send — each has its own run, its own status line and its own Stop, and the conversation list marks the ones working with *Working*. Two conversations can run on the same project. A file one of them is changing is reserved to it until its run ends: the other is refused, naming the conversation, and can wait or work elsewhere (reading is never blocked). Services and debug sessions belong to the conversation that started them. Builds queue. See [Imber › Running and observing](06-running-and-observing.md).
