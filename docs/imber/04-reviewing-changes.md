---
title: "Imber: Reviewing changes"
summary: "Proposals, outdated proposals, Undo, Local History and the isolated copy with Apply."
keywords: ["proposal", "diff", "apply", "reject", "undo", "outdated", "local history", "isolated copy", "worktree"]
related: ["Imber: Permissions and modes", "Imber: Conversations", "Imber: Tools it can use"]
source: src/lib/components/bennu/docs/imber/Review.svelte
---
<!-- Generated from the in-app docs - do not edit. Edit the Svelte pages and run `node scripts/build-help-index.mjs`. -->

# Imber: Reviewing changes

Whatever Imber changes can be read first, written all-or-nothing, undone afterwards, and recovered from Local History; risky work can happen in an isolated copy that touches nothing until you apply it.

## Proposals

In Manual mode a change is a **proposal**: a card in the thread listing the files with their added and removed lines. Nothing is written yet. A change made by one tool call (an edit across files, a rename, a replace over a folder) is one proposal, written all or nothing.

**Review** opens the proposal as a diff, unified or side by side, with the files in a list on the left (`↑` / `↓` move between them). Decide with **Apply all** (`Ctrl+Enter`) or **Reject all**. A rejected card turns red and keeps its files readable; an applied one stays as a record and can be undone. A proposal that touches a file of another project names it as `project:path`.

## Applied changes and Undo

In Accept edits and Autonomous, edits inside the reach are applied as they are proposed. The card stays as a record and has **Undo**: it puts back exactly the files that proposal wrote, all of them or none, and refuses if one of the files changed since. Every write is also recorded in Local History.

## Outdated proposals

A proposal is applied as a whole. Bennu first checks that every file is still as it was when Imber read it. If any changed (you typed, or another proposal went in first) *nothing* is written and the card turns **Outdated**, marking the files that changed. It stays in the conversation with three ways forward:

- **Re-apply on current file** (`Ctrl+Enter` in the review window): makes each change again on the file as it is now, when its lines are still there exactly once, and gives you the result to approve.
- **Ask to redo**: drops it and asks Imber, in the conversation, to propose it again.
- **Discard**: drops it.

If a write fails after others went in, those files are put back, so a proposal is never half applied. A file that another undecided proposal also edits is flagged on the card: applying either makes the other outdated.

## Local History

Everything Imber writes in the project is recorded in Bennu's Local History, deletions included (also the ones applied from an isolated copy). Imber can list a file's revisions, show what changed, list deleted files and restore one; a restore is an edit like any other, proposed or applied by your mode. You can do the same yourself from the editor; see Local history.

## Isolated copy

The composer's settings menu has *Isolated copy*, off by default and fixed when a conversation starts. Edits land in the copy at once and nothing reaches your real files until you apply.

| Kind | What it is | What works |
| --- | --- | --- |
| **Copy — text only** | In a git repository (with git access on) a detached git worktree, so nothing is copied; otherwise a plain copy of the sources, capped at 512 MB or 40,000 files, leaving out version control, dependency folders and build output. | Files, search, build, run and live tools. No code intelligence (usages, symbols, problems, rename, beans) and no indexing, so it is cheap. |
| **Git worktree — indexed** | A real worktree opened and indexed as a hidden project. Offered only for a git repository with git access on; never appears in your projects; shows its indexing progress in the conversation. | Every tool. |

- A worktree starts from what you see: files you had changed, staged or not yet added are copied over it.
- Both live in the app data folder, never beside the repository. While one is active a banner says so.
- **Apply to project** (`Ctrl+Shift+Enter` with focus in the panel, or *Imber: Review and apply the isolated copy*) shows the combined diff first. Applying is all or nothing: if you, another conversation or a tool changed a file in the project that the copy also changed, nothing is written and each such file is listed with the reason. Deletions and renames are included and the written files are recorded in Local History.
- **Discard** (*Imber: Discard the isolated copy…*) removes the copy, its index and the processes started in it. Deleting the conversation does too. A copy left behind by a crash, or unused for 30 days beyond the 12 most recent, is swept at startup.
- Services started inside a copy are not shown in the Live panel of the project. A conversation in a copy has its own tree and never queues against the project's builds.
- You cannot apply or discard while Imber is still working in the copy: stop it first.

Start one from the composer's menu or the palette: *Imber: New conversation in an isolated copy (text only)*, *Imber: New conversation in a git worktree (indexed)*.
