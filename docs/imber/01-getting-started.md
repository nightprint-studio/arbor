---
title: "Imber: Getting started"
summary: "Open the panel, store the Anthropic API key, choose a model and effort, send a first request."
keywords: ["api key", "anthropic", "model", "effort", "composer", "home", "first request", "project instructions", "dry run"]
related: ["Imber: Settings reference", "Imber: Conversations", "Imber: Cost and context"]
source: src/lib/components/bennu/docs/imber/GettingStarted.svelte
---
<!-- Generated from the in-app docs - do not edit. Edit the Svelte pages and run `node scripts/build-help-index.mjs`. -->

# Imber: Getting started

Open the panel with `Alt+A`, store your Anthropic API key once in Settings, pick a model, and send a request.

## Open the panel

The panel sits on the right of the editor. Open it with `Alt+A`, the sparkle on the right rail, or *Toggle Imber* in the Command Palette. With no conversation open it shows its **home**: the mode you are in, the project, the file and the instruction files it would work with, a few starters (an agent card sends a request at once; a chat card leaves the start of a sentence in the composer) and your recent conversations. The panel opens on it.

## Set the API key

> **You need an Anthropic API key** Imber is billed to your own Anthropic account. Every message costs money, and the conversation shows what it has cost.

1. Open **Settings › Imber** (the **…** menu of the panel has *Imber settings…*).
2. Paste the key under **Connection › API key** and press **Save**. The page then shows *Set*; it never shows the key again.
3. **Remove** deletes it from the vault.

The key is kept in a credential vault of its own, apart from the one that holds your git tokens, is never written to a config file, and is sent only to Anthropic with each request. Without a key a request fails with "No API key is set. Add one in Settings › Imber."

## Choose a model, effort and window

The composer has one quiet line under the text box:

- **Model**: each entry shows what it is for, the size of its window and its price per million tokens (input / output). The list and the prices come from a file; see [Imber › Cost and context](13-cost-and-context.md).
- **Settings** (the sliders icon): *Effort* (how hard it thinks: Low, Medium, High, Extra high, Max; a model without such a setting ignores it), *Context window* (the largest window Imber may use, 50K up to 1M, limited by the model), *Reach*, *Permissions*, *Decisions*, *Skills…*, *Step plan*, *Sub-agents*, *Isolated copy* and *Build isolation*. Each shows its current value.
- **Mode** (agent conversations only): Manual, Accept edits or Autonomous. See [Imber › Permissions and modes](03-permissions.md).

The defaults for a *new* conversation are set in Settings › Imber › New conversations: Opus 5.5 at High effort with a 200K window, in Manual mode with the reach "this project". The composer changes them for the conversation you are in. The full list is in [Imber › Settings reference](15-settings.md).

## Send a first request

1. Open a project (Imber works inside one) and the file you care about. The file you are in and the lines you selected travel with the message, shown as chips above the text.
2. Type a request, for example "Explain what this class does" or "Rename `total` to `amountDue` everywhere".
3. Press `Ctrl+Enter` to send; `Enter` is a new line.
4. **Stop** (the square where Send was) ends a run; what it had written stays.

In Manual mode a change arrives as a proposal card; open *Review* to read the diff and **Apply all** (`Ctrl+Enter` in the review window) or **Reject all**. See [Imber › Reviewing changes](04-reviewing-changes.md).

## See the request before it is sent

The **…** menu has *Show what would be sent…*, which opens the exact request for the next message without sending it, and *Dry run*, which makes Send show the request instead of making it. Both list where the size comes from and a rough cost. The key is masked.

## Project instructions are read automatically

The project's `CLAUDE.md`, `.claude/CLAUDE.md` and `AGENTS.md`, and your own `assistant-instructions.md` in Bennu's config folder, are added to Imber's instructions, so it follows the build command, conventions and no-go areas a team has written down. Each file is cut at 16 KB and all together at 40 KB, and a file whose text repeats another is sent once. They appear as chips above the text. They never override the access prompts or the ban on a shell. The switch is Settings › Imber › New conversations › Project instructions.
