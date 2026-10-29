---
title: "Imber: Decisions and questions"
summary: "How much Imber decides alone (ask at forks, only big decisions, decide itself) and how its questions work."
keywords: ["decision", "ask_user", "question", "notes", "ask at forks", "decide itself"]
related: ["Imber: Permissions and modes", "Imber: Settings reference"]
source: src/lib/components/bennu/docs/imber/Decisions.svelte
---
<!-- Generated from the in-app docs - do not edit. Edit the Svelte pages and run `node scripts/build-help-index.mjs`. -->

# Imber: Decisions and questions

The decision level sets how much Imber chooses alone when a choice is yours; when it does ask, it asks with a card of choices in the thread.

## The decision level

| Level | Behaviour |
| --- | --- |
| **Ask at forks** (default) | It stops to ask when a choice is yours. |
| **Only big decisions** | It asks about architecture, new dependencies and anything hard to undo, and decides the rest. |
| **Decide itself** | It picks the best option, asks only when truly blocked (at most two questions a run) and lists its decisions at the end. |

Set the default in **Settings › Imber › New conversations › Decisions**. A conversation can follow the setting or have its own level: composer settings menu › *Decisions*, or *Imber: Decisions in this conversation…* in the Command Palette. A change applies from the next message.

## Your own words about decisions

Under the level there is a free-text field ("Your instructions about decisions", up to 2,000 characters) for your own rules at any level, for example "always ask before touching public API" or "never add a dependency". A conversation can add its own text in the same window; it is appended to the global one.

The same window also holds the conversation's own switch for *Start a run when the program stops* (see [Imber › Debugging with Imber](07-debugging.md)).

## Questions

Where it would otherwise guess, Imber asks with `ask_user`. Each question is a column of choices, one per row: pick one, or tick several, and *Other…* is always there to type your own. When it asks several things at once, a strip of tabs names them (a tick once answered): jump to any, or use **Back** and **Next**; the last step sends all the answers together, so changing an earlier one costs nothing. Answered, the card shrinks to what you decided and stays in the thread.

Questions about access or confirmation ("Allow this?", "Start a process?") are not part of this: they follow the mode and reach, see [Imber › Permissions and modes](03-permissions.md). Sub-agents cannot ask you questions.
