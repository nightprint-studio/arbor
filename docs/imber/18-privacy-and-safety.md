---
title: "Imber: Privacy and safety"
summary: "What leaves the machine, what is masked, what is never granted, and the safeguards in place."
keywords: ["privacy", "security", "secrets", "api key", "masked", "authors", "never", "anthropic"]
related: ["Imber: Permissions and modes", "Imber: Tools it can use"]
source: src/lib/components/bennu/docs/imber/Privacy.svelte
---
<!-- Generated from the in-app docs - do not edit. Edit the Svelte pages and run `node scripts/build-help-index.mjs`. -->

# Imber: Privacy and safety

Imber sends to Anthropic what it reads in order to answer; the API key, secret files and other people's names get special treatment, and a short list of things is never allowed whatever you click.

## What leaves your machine

Each request goes to Anthropic's API, over the key you stored, and nothing goes anywhere else. A request contains:

- Imber's instructions and the definitions of the tools it was offered;
- your project instruction files (`CLAUDE.md`, `AGENTS.md`, your own instructions file) if that setting is on, and the one-line index of the skills you enabled;
- the whole conversation: your messages, Imber's answers and the results of every tool it called;
- the open file's path and the lines you selected, with your message.

**Tool results are the part to keep in mind.** A file Imber reads, a search result, build and test output, logs, a debugger value, a Bevy component or a page screenshot is part of the conversation and is sent, and sent again whenever the conversation is re-read. Files it does not read are not sent. In a chat only what you type, paste or select is sent.

To see exactly what goes, use *Show what would be sent…* or *Dry run* in the panel's **…** menu: both show the request body and its size, with the key masked.

## The API key

Stored in a credential vault of its own, apart from the one holding your git tokens; never written to a config file; never held by the frontend after you typed it (the Settings page only learns whether one is stored); read only by the backend when a request is made and sent only to Anthropic.

## What is masked or replaced

- **Authors in git results**: with Hide authors on (the default) people appear as ids such as `Author-7f3a9c`. The mapping stays on this machine and is used to turn ids back into names for you.
- **Debugger values** of variables named like secrets (password, token, secret, apiKey, credential…) are masked before Imber sees them.
- **Secret files** (`.env`, key files) are not read without your yes each time.
- **The key** in the request preview.

Masking is by name and pattern. It is not a guarantee that a secret pasted into a message, or sitting inside a file with an ordinary name, is hidden: what you ask Imber to read, it reads.

## What is never granted

- A shell, or any command Imber typed itself.
- Writing a secret file, or anything under `.git`.
- Pushing to a remote. A commit always asks.
- Reaching anything but this machine over HTTP, for requests and screenshots and the Bevy connection.
- Deleting a whole project or allowed folder, or anything outside the reach, without asking.
- Changing a running program or Bevy game without the separate switch and a yes per conversation. Autonomous mode does not waive either.
- Running the commands inside a skill: skills are text.

## Safeguards you can lean on

- Spending limits stop runaway runs in every mode.
- Edits are all-or-nothing, can be undone, and are recorded in Local History.
- Concurrent conversations cannot write the same file at once.
- Sub-agents never have more access than the conversation, and cannot start agents.
- An isolated copy keeps every change away from your files until you apply it.

The Bennu help Imber consults is built into the application; looking things up in it sends nothing by itself, though the answer becomes part of the conversation.
