---
title: "Imber: Keyboard and palette"
summary: "Shortcuts and Command Palette verbs for Imber."
keywords: ["shortcut", "keyboard", "palette", "alt+a", "ctrl+shift+q", "ctrl+shift+u", "command palette"]
related: ["Imber: Permissions and modes", "Imber: Working from the editor"]
source: src/lib/components/bennu/docs/imber/Shortcuts.svelte
---
<!-- Generated from the in-app docs - do not edit. Edit the Svelte pages and run `node scripts/build-help-index.mjs`. -->

# Imber: Keyboard and palette

Everything Imber does can be driven from the keyboard: a handful of shortcuts, and a family of Command Palette verbs that all start with "Imber:".

## Shortcuts

| Keys | Where | What it does |
| --- | --- | --- |
| `Alt+A` | Bennu window | Open or close the Imber panel. |
| `Ctrl+Enter` | Composer | Send the message. `Enter` is a new line. |
| `Ctrl+Shift+U` | Bennu window, panel on screen | Expand the panel over the editor, or restore it. `Esc` inside the panel (with nothing open over it) also restores it. |
| `Ctrl+Shift+Q` | Focus inside the panel | Next mode: Manual, Accept edits, Autonomous. |
| `Ctrl+Shift+Enter` | Focus inside the panel, isolated copy active | Review and apply the isolated copy. |
| `Ctrl+Shift+V` | Composer | Paste exactly what is on the clipboard, with no code fence. |
| `/` | Start of the composer | Pick one of the enabled skills. |
| `↑` / `↓` | Review window | Move between the files of a proposal. |
| `Ctrl+Enter` | Review window | Apply all (or, on an outdated proposal, re-apply on the current file). |

## Command Palette

| Verb | Shown when |
| --- | --- |
| *Toggle Imber* | always |
| *Ask Imber about this…* | in an editor: opens the margin menu at the caret |
| *Imber: Expand panel* / *Imber: Restore panel* | depending on the state of the panel |
| *Imber: Retry the failed request* | when the last answer ended in an error |
| *Imber: Retry now (skip the wait)* | while a request waits to be sent again |
| *Imber: Manage skills…* | always |
| *Imber: Mode — Manual* / *Accept edits* / *Autonomous* | for the modes you are not in |
| *Imber: Next mode (Manual → Accept edits → Autonomous)* | always |
| *Imber: Decisions in this conversation…* | always |
| *Imber: Sub-agents in this conversation — next* | in an agent conversation |
| *Imber: Open the latest sub-agent's thread…* | when the conversation has sub-agents |
| *Imber: Stop all sub-agents* | while sub-agents are running |
| *Imber: New conversation in an isolated copy (text only)* | always |
| *Imber: New conversation in a git worktree (indexed)* | always |
| *Imber: Review and apply the isolated copy* (two entries, one with the shortcut and one that also shows the panel) | while a copy is active |
| *Imber: Discard the isolated copy…* | while a copy is active |

Shortcuts that are not Imber's own but matter when working with it: `Ctrl+Shift+F8` opens the breakpoints list (the ones Imber set are badged), and `Ctrl+Shift+F11` opens Live.

Without a mouse: open the panel with `Alt+A`, type, `Ctrl+Enter`; review with the keys above; change mode with `Ctrl+Shift+Q` or the palette verbs above.
