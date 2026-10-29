---
title: "Imber: the assistant in Bennu"
summary: "What Imber is, how it works in one page, and a map of every Imber page."
keywords: ["imber", "assistant", "ai", "agent", "chat", "claude", "overview"]
related: ["Imber: Getting started", "Imber: Permissions and modes", "Imber: Cost and context"]
source: src/lib/components/bennu/docs/imber/Overview.svelte
---
<!-- Generated from the in-app docs - do not edit. Edit the Svelte pages and run `node scripts/build-help-index.mjs`. -->

# Imber: the assistant in Bennu

Imber is the AI assistant built into Bennu: a chat and agent panel on the right of the editor that reads, searches, edits, builds, runs and debugs your project through Bennu's own tools, and asks before anything that matters.

## What Imber is

Imber runs on Claude, a model made by Anthropic, and talks to it with your own Anthropic API key. It is not a separate product: it lives in the Bennu window (`Alt+A`, or the sparkle on the right rail) and works on the project that is open. The name is Latin for *rain*: the thing the whole forest of Arbor runs on.

There are two kinds of conversation, kept apart by the **Agent / Chat** switch in the panel header. An *agent* conversation works inside the project with Bennu's tools. A *chat* conversation has no tools on the project at all: it only sees what you type or select, and can look up Bennu's own documentation, open a panel and draw a diagram.

## The ideas behind it

- **Bennu's tools, never a shell.** Imber reads and changes the project through the same engine you use: the index, usages, rename, structural replace, build, tests, Local History. It cannot type arbitrary commands.
- **You decide how much it decides.** Three modes (Manual, Accept edits, Autonomous), four reaches (this file, this project, all open projects, anywhere on disk) and a matrix of what asks when. See [Imber › Permissions and modes](03-permissions.md).
- **Everything it changes can be reviewed and undone.** Proposals, a diff window, Undo on applied changes, Local History, and an isolated copy for risky work. See [Imber › Reviewing changes](04-reviewing-changes.md).
- **The cost is visible and capped.** Every conversation shows what it cost, and limits in euros stop a runaway run. See [Imber › Cost and context](13-cost-and-context.md).
- **Your key stays on your machine.** It lives in a credential vault of its own and goes only to Anthropic. See [Imber › Privacy and safety](17-privacy-and-safety.md).

## Where to read next

| Page | Read it to learn |
| --- | --- |
| **[Imber › Getting started](01-getting-started.md)** | Opening the panel, setting the key, choosing a model, sending a first request. |
| **[Imber › Conversations](02-conversations.md)** | History, renaming, deleting, Agent versus Chat, the home screen, the step plan. |
| **[Imber › Permissions and modes](03-permissions.md)** | The single table of what asks, by mode and by action. |
| **[Imber › Reviewing changes](04-reviewing-changes.md)** | Proposals, outdated proposals, Undo, Local History, the isolated copy. |
| **[Imber › Tools it can use](05-tools.md)** | Every family of tools, how to switch one off, git, several projects. |
| **[Imber › Running and observing](06-running-and-observing.md)** | Processes, recipes, scenarios, logs, the build queue and who broke the build. |
| **[Imber › Debugging with Imber](07-debugging.md)** | Breakpoints, stops, snapshots and the tools that change a running program. |
| **[Imber › Bevy games and the profiler](08-bevy-and-profiler.md)** | Looking into a running Bevy game and reading a profiler recording. |
| **[Imber › Screenshots and diagrams](09-screenshots-and-diagrams.md)** | Seeing a front end on localhost, and drawing Mermaid diagrams. |
| **[Imber › Skills](10-skills.md)** | Teaching Imber how you want a kind of task done. |
| **[Imber › Sub-agents](11-sub-agents.md)** | Helpers with a clean context for side tasks. |
| **[Imber › Decisions and questions](12-decisions-and-questions.md)** | How much it decides alone and how it asks you. |
| **[Imber › Cost and context](13-cost-and-context.md)** | Tokens, the cache, compaction, limits in euros. |
| **[Imber › Working from the editor](14-editor.md)** | The sparkle in the margin, comments, stack traces, pasted code, the expanded panel. |
| **[Imber › Settings reference](15-settings.md)** | Every setting: what it does, its default, where it is stored. |
| **[Imber › Keyboard and palette](16-keyboard-and-palette.md)** | Shortcuts and Command Palette verbs. |
| **[Imber › Privacy and safety](17-privacy-and-safety.md)** | What leaves the machine, what is masked, what is never allowed. |
| **[Imber › Troubleshooting](18-troubleshooting.md)** | Common messages and what to do about them. |

## Pages in other parts of the manual

Some topics belong to a feature and live with it: **Live** (processes, recipes, scenarios, the timeline and the build queue), **Debugging**, **Bevy Remote** and **Profiler**. Each has a short section for Imber and the Imber pages above link to them. **Voice** (in the Editor group) covers dictating to Imber by voice, the voice overlay, the speech models and the wake word.
