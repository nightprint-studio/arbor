# Imber documentation

<!-- Generated from the in-app docs - do not edit. Edit the Svelte pages and run `node scripts/build-help-index.mjs`. -->

> Imber is the AI assistant built into the Bennu code editor (part of the Arbor suite). It runs on Claude with the user's own Anthropic API key and works on the open project through Bennu's own tools. These pages describe how it behaves, what it asks before doing, and every setting.

## Contents
- Imber: the assistant in Bennu: What Imber is, how it works in one page, and a map of every Imber page.
- Imber: Getting started: Open the panel, store the Anthropic API key, choose a model and effort, send a first request.
- Imber: Conversations: Saved conversations, rename and delete, Agent versus Chat, the home screen, the step plan.
- Imber: Permissions and modes: Manual, Accept edits and Autonomous modes, the four reaches, and one table of what asks when.
- Imber: Reviewing changes: Proposals, outdated proposals, Undo, Local History and the isolated copy with Apply.
- Imber: Tools it can use: Every family of tools with its purpose, how to switch one off, git access and several projects.
- Imber: Running and observing: Managed processes, cargo run, the programs you set up, recipes, scenarios, requests, log reading, the shared build queue and build-error attribution.
- Imber: Debugging with Imber: Breakpoints, launching and attaching, stops and snapshots, and the off-by-default tools that change a running program.
- Imber: Bevy games and the profiler: Looking into a running Bevy game over BRP, changing it behind a separate switch, and reading a profiler recording.
- Imber: Screenshots and diagrams: Seeing a front end on localhost and drawing Mermaid diagrams in the thread.
- Imber: Skills: SKILL.md folders that teach Imber a kind of task: format, locations, the manager, limits.
- Imber: Sub-agents: Clean-context helpers for side tasks: when to use them, permissions, cost cap, how to watch and stop them.
- Imber: Decisions and questions: How much Imber decides alone (ask at forks, only big decisions, decide itself) and how its questions work.
- Imber: Schedules: Ask Imber something on a timetable: how to create a schedule, what a scheduled run may do, missed runs, cost limits and the MCP tools.
- Imber: Cost and context: Context meter, models and prices, cache, compaction, confirm-on-change and spending limits in euros.
- Imber: Working from the editor: The sparkle in the margin, doc comments, Markdown selections, stack traces, pasted code and the expanded panel.
- Imber: Settings reference: Every Imber setting: what it does, its default, the config key and whether a conversation can override it.
- Imber: Keyboard and palette: Shortcuts and Command Palette verbs for Imber.
- Imber: Privacy and safety: What leaves the machine, what is masked, what is never granted, and the safeguards in place.
- Imber: Troubleshooting: Common messages and what to do about them.

---

# Imber: the assistant in Bennu

Imber is the AI assistant built into Bennu: a chat and agent panel on the right of the editor that reads, searches, edits, builds, runs and debugs your project through Bennu's own tools, and asks before anything that matters.

## What Imber is

Imber runs on Claude, a model made by Anthropic, and talks to it with your own Anthropic API key. It is not a separate product: it lives in the Bennu window (`Alt+A`, or the sparkle on the right rail) and works on the project that is open. The name is Latin for *rain*: the thing the whole forest of Arbor runs on.

There are two kinds of conversation, kept apart by the **Agent / Chat** switch in the panel header. An *agent* conversation works inside the project with Bennu's tools. A *chat* conversation has no tools on the project at all: it only sees what you type or select, and can look up Bennu's own documentation, open a panel and draw a diagram.

## The ideas behind it

- **Bennu's tools, never a shell.** Imber reads and changes the project through the same engine you use: the index, usages, rename, structural replace, build, tests, Local History. It cannot type arbitrary commands.
- **You decide how much it decides.** Three modes (Manual, Accept edits, Autonomous), four reaches (this file, this project, all open projects, anywhere on disk) and a matrix of what asks when. See Imber › Permissions and modes.
- **Everything it changes can be reviewed and undone.** Proposals, a diff window, Undo on applied changes, Local History, and an isolated copy for risky work. See Imber › Reviewing changes.
- **The cost is visible and capped.** Every conversation shows what it cost, and limits in euros stop a runaway run. See Imber › Cost and context.
- **Your key stays on your machine.** It lives in a credential vault of its own and goes only to Anthropic. See Imber › Privacy and safety.

## Where to read next

| Page | Read it to learn |
| --- | --- |
| **Imber › Getting started** | Opening the panel, setting the key, choosing a model, sending a first request. |
| **Imber › Conversations** | History, renaming, deleting, Agent versus Chat, the home screen, the step plan. |
| **Imber › Permissions and modes** | The single table of what asks, by mode and by action. |
| **Imber › Reviewing changes** | Proposals, outdated proposals, Undo, Local History, the isolated copy. |
| **Imber › Tools it can use** | Every family of tools, how to switch one off, git, several projects. |
| **Imber › Running and observing** | Processes, recipes, scenarios, logs, the build queue and who broke the build. |
| **Imber › Debugging with Imber** | Breakpoints, stops, snapshots and the tools that change a running program. |
| **Imber › Bevy games and the profiler** | Looking into a running Bevy game and reading a profiler recording. |
| **Imber › Screenshots and diagrams** | Seeing a front end on localhost, and drawing Mermaid diagrams. |
| **Imber › Skills** | Teaching Imber how you want a kind of task done. |
| **Imber › Sub-agents** | Helpers with a clean context for side tasks. |
| **Imber › Decisions and questions** | How much it decides alone and how it asks you. |
| **Imber › Cost and context** | Tokens, the cache, compaction, limits in euros. |
| **Imber › Working from the editor** | The sparkle in the margin, comments, stack traces, pasted code, the expanded panel. |
| **Imber › Settings reference** | Every setting: what it does, its default, where it is stored. |
| **Imber › Keyboard and palette** | Shortcuts and Command Palette verbs. |
| **Imber › Privacy and safety** | What leaves the machine, what is masked, what is never allowed. |
| **Imber › Troubleshooting** | Common messages and what to do about them. |

## Pages in other parts of the manual

Some topics belong to a feature and live with it: **Live** (processes, recipes, scenarios, the timeline and the build queue), **Debugging**, **Bevy Remote** and **Profiler**. Each has a short section for Imber and the Imber pages above link to them. **Voice** (in the Editor group) covers dictating to Imber by voice, the voice overlay, the speech models and the wake word.

---

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

- **Model**: each entry shows what it is for, the size of its window and its price per million tokens (input / output). The list and the prices come from a file; see Imber › Cost and context.
- **Settings** (the sliders icon): *Effort* (how hard it thinks: Low, Medium, High, Extra high, Max; a model without such a setting ignores it), *Context window* (the largest window Imber may use, 50K up to 1M, limited by the model), *Reach*, *Permissions*, *Decisions*, *Skills…*, *Step plan*, *Sub-agents*, *Isolated copy* and *Build isolation*. Each shows its current value.
- **Mode** (agent conversations only): Manual, Accept edits or Autonomous. See Imber › Permissions and modes.

The defaults for a *new* conversation are set in Settings › Imber › New conversations: Opus 5.5 at High effort with a 200K window, in Manual mode with the reach "this project". The composer changes them for the conversation you are in. The full list is in Imber › Settings reference.

## Send a first request

1. Open a project (Imber works inside one) and the file you care about. The file you are in and the lines you selected travel with the message, shown as chips above the text.
2. Type a request, for example "Explain what this class does" or "Rename `total` to `amountDue` everywhere".
3. Press `Ctrl+Enter` to send; `Enter` is a new line.
4. **Stop** (the square where Send was) ends a run; what it had written stays.

In Manual mode a change arrives as a proposal card; open *Review* to read the diff and **Apply all** (`Ctrl+Enter` in the review window) or **Reject all**. See Imber › Reviewing changes.

## See the request before it is sent

The **…** menu has *Show what would be sent…*, which opens the exact request for the next message without sending it, and *Dry run*, which makes Send show the request instead of making it. Both list where the size comes from and a rough cost. The key is masked.

## Project instructions are read automatically

The project's `CLAUDE.md`, `.claude/CLAUDE.md` and `AGENTS.md`, and your own `assistant-instructions.md` in Bennu's config folder, are added to Imber's instructions, so it follows the build command, conventions and no-go areas a team has written down. Each file is cut at 16 KB and all together at 40 KB, and a file whose text repeats another is sent once. They appear as chips above the text. They never override the access prompts or the ban on a shell. The switch is Settings › Imber › New conversations › Project instructions.

---

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
- Cards for proposals (Imber › Reviewing changes), questions, access prompts, the step plan, sub-agents, live runs and debugger stops.
- Notices in the thread: a compaction, a limit reached, an access grant, a system note about another conversation.

## The step plan

For work with several steps Imber writes a plan first and ticks the steps off. The card lists them (done, in progress, still to do) with a bar and "3 of 7 · 43%". The percentage is steps done over steps planned: it says how much of the list is done, not how long the rest will take, and it goes down if the plan grows. The plan costs a few hundred output tokens per update, a few cents for a task of several steps. Questions and small changes get no plan. **Step plan** in the composer's settings menu (and in Settings › Imber) switches it off; the tool and its instruction are then not sent at all. The plan is also part of what Imber reads back: after a stop, a *continue* or a compaction it still has the latest plan, word for word, and a new message in a conversation whose plan is still open reminds it where the plan got to. While it works, a plan left untouched for five tool calls is recalled to it with the step in progress, so steps are ticked as they are done; and if it ends its answer with steps still open, it is asked once to tick them or to say what is left.

## The strip on the message box

A strip on top of the message box says what the conversation has going on, so you never scroll back to find a card. **Plan** shows the steps done (*3/6*) and names the step being worked on, with the plan's progress as a line along the strip's foot; **Agents** says how many are running, and **Review** how many proposals wait for your decision. Each segment is there only when it has something to show. Press one to open its panel over the thread — every step of the plan, with the one in progress brought to the middle and marked *Now*; every sub-agent with its Stop; every proposal with *Review…*. Press it again, Esc or a click outside closes it. The command palette opens the same panels (*Imber: Show the plan*, *Show the sub-agents*, *Show the edits to review*).

## Writing while it works

You do not have to wait or stop Imber to say something. While it works, write in the message box and press `Ctrl+Enter` (or the arrow beside Stop): the message waits above the box, marked *Waiting*, and Imber reads it before its next request — after the tool it is running, without stopping. In the thread it then stands where it was read, as *You, while it worked*. Words only: files go with your next ordinary message. If the run ends before reading it, the message comes back into the box for you to send.

## Steps and non-stop

A run makes at most **Steps per run** requests to the model (Settings › Imber › Conversations, 40 by default; a step is one request, however many tools it calls). When it gets there it stops with “Stopped after N steps” and waits for you to say *continue*, which picks up where it was.

**Non-stop**, in the composer's settings menu or the command palette (*Imber: Non-stop in this conversation*), lifts that cap for one conversation: its runs go on until the work is done or you stop them. The spending limits still stop it, and a scheduled run in that conversation keeps the cap. The choice is kept with the conversation. While it is on, the settings button carries a small ∞ and the menu's row reads *On*.

## Several conversations at once

Several conversations can work at the same time: start one, open another and send — each has its own run, its own status line and its own Stop, and the conversation list marks the ones working with *Working*. Two conversations can run on the same project. A file one of them is changing is reserved to it until its run ends: the other is refused, naming the conversation, and can wait or work elsewhere (reading is never blocked). Services and debug sessions belong to the conversation that started them. Builds queue. See Imber › Running and observing.

---

# Imber: Permissions and modes

Two settings decide what Imber may do without asking: the **mode** (how much it decides for itself) and the **reach** (where it may go). The table below answers "what asks, when" for every action.

## The three modes

Chosen from the mode menu in the composer, with `Ctrl+Shift+Q` (next mode, while focus is in the panel), from the Command Palette (*Imber: Mode — …*, *Imber: Next mode*) and, as the default for new conversations, in Settings › Imber › New conversations › Mode.

- **Manual**: every edit is a proposal you approve (review the diff, apply or reject). Deleting and building ask. Anything outside what you allowed asks.
- **Accept edits**: edits and creations inside what you allowed are applied as they are proposed, and each can be undone from its card. Deleting inside the reach does not ask. The project's own build and tests ask once per run.
- **Autonomous**: the same as Accept edits, plus no question for the project's own build and tests, plus one extra power: it may open for reading and writing a file that an error of its own build names (see below).

A saved mode named *Auto* from an older setup reads as Accept edits.

## The four reaches

The shield beside *Add context*, or Reach in the composer's settings menu, sets where Imber may read and change without asking. The default for new conversations is in Settings › Imber › What it may reach.

| Reach | What it covers |
| --- | --- |
| **This file** | Only the file you are in. No listing and no creating beside it. A tool that answers about the whole project (usages, callers, the framework catalogue) is held to the project folder, so it asks. |
| **This project** (default) | Every file and every Bennu tool of the project. |
| **All open projects** | The projects open in the workspace, the one you are in first, up to Settings › Imber › Projects at once (6 by default). |
| **Anywhere on disk** | No access prompts at all. Reading a secret, deleting and the hard limits below still apply. |

Always allowed on top of the reach: the **extra folders** you added in Settings › Imber › What it may reach, and its own **Temp area** (scratch files and downloaded logs) when *Use the Temp folders* is on. **Edit files** off makes Imber read-only: every write is refused with "editing is switched off".

## What asks, when

"Inside" means inside the reach, the extra folders or the Temp area. Cells say what happens; *asks* means a yes/no prompt in the thread, *proposal* means a card you review.

| Action | Manual | Accept edits | Autonomous |
| --- | --- | --- | --- |
| **Read, search, project intelligence (inside)** | Free | Free | Free |
| **Edit an existing file (inside)** | Proposal | Applied at once, Undo on the card | Applied at once, Undo on the card |
| **Create a file (inside)** | Proposal | Applied at once | Applied at once |
| **Rename a symbol, add a parameter, replace across files, structural replace, restore from Local History** | Proposal | Applied, recorded in Local History | Applied, recorded in Local History |
| **Delete (inside the reach)** | Asks | Free, but a call removing more than 50 files asks | Same as Accept edits |
| **Delete in the Temp area** | Free | Free | Free |
| **Delete outside the reach, or a whole project or allowed folder** | Asks | Asks | Asks |
| **Check, build or run the tests of the project (`bennu_check`, `bennu_build`, `bennu_test_run`)** | Asks on every call | Asks once per run | Free |
| **Other things that execute code (cargo run, npm scripts, run configurations, launching a program)** | Asks | Asks | Asks |
| **Other Bennu tools that change something** | Asks | Free | Free |
| **Start a process (`run_start`)** | Asks every time | Asks every time | Asks every time |
| **Run a Rust binary or example (`cargo_run`)** | Asks the first time for each target in a conversation; then free with any features, arguments or environment | Same | Same |
| **Run a program you set up (`run_tool`)** | As the tool says (every call, once per conversation, never); every call when the tool changes things | As the tool says | As the tool says |
| **Run a script of the project that is not one of your programs (`run_script`)** | Asks every time, showing the script: decline, run once, or run and add it to your programs (then it is asked about once per conversation). A script added this way asks again when its content changes | Same | Same |
| **Run a recipe or a scenario** | Asks once per content of the recipes file or scenario | Same | Same |
| **HTTP request to localhost** | GET, HEAD, OPTIONS free; any other method asks unless the port belongs to a process it started | Same | Same |
| **Debugger: set breakpoints, read the stack and values, step** | Free | Free | Free |
| **Debugger: launch or attach** | Asks every time | Asks every time | Asks every time |
| **Debugger: change the running program (throw, return early, drop a frame, set a value)** | Off by default; when on, asks once per conversation | Same | Same: Autonomous does not skip it |
| **Bevy: read a running game** | Free (this machine only) | Free | Free |
| **Bevy: change the running game** | Off by default; when on, asks once per conversation | Same | Same |
| **Screenshot of a localhost page** | Free | Free | Free |
| **Git: read (status, diff, log, blame)** | Free once git is switched on | Free | Free |
| **Git: stage, unstage** | Asks | Free | Free |
| **Git: commit** | Asks | Asks | Asks. Push does not exist |
| **Bring in another project for Bennu's tools** | Open in Bennu: free inside the reach, the access card outside it. Not open: the access card outside the reach (a yes also indexes it); inside it, as *Opening other projects* says (asks by default) | Same | Same, except that by default a project not open in Bennu but inside the reach opens without asking |
| **A path outside the reach** | One prompt: Deny, Allow reading, Allow reading and writing, for that folder, for the session. It says what the call wants there (read, search, change, create, delete), and the grant that lets it go on is the highlighted button | Same | Same, except the build-error case below. "Anywhere on disk" never asks |
| **A file named by an error of Imber's own build or check (inside the project)** | The prompt says the build reported an error there | Same | Opened for reading and writing without a prompt, for that run, shown with a Revoke button |
| **Read a secret file (`.env`, keys)** | Asks every time | Asks every time | Asks every time |
| **Write a secret file, or anything under `.git`** | Refused | Refused | Refused |
| **Ask you a question (`ask_user`)** | Always possible; see Imber › Decisions and questions | Same | Same |

A grant you give (reading, or reading and writing) lasts for the session and applies to that folder. A sub-agent's grants are its own and do not carry back to the conversation; see Imber › Sub-agents.

## Autonomous and the build-error files

Under Autonomous, a file named by an *error* of a build or check Imber ran itself (`bennu_check`, `bennu_build`, `bennu_check_file`, `bennu_problems`) and located inside the project you are in is opened for reading and writing, for that run only, without a prompt. Limits: at most 25 files a run; never anything under build output or fetched code (`target`, `build`, `out`, `dist`, `node_modules`, generated folders); never a dependency manifest or lockfile (`pom.xml`, `Cargo.toml`, `package.json`); never a secret. The thread shows each one ("Access granted to … because the build reports an error there") with a *Revoke* button, and the access ends with the run. Files of another project open this way only when the reach is *all open projects*; otherwise it asks. In Manual and Accept edits the prompt for such a file says why it is being asked.

## Revoking access

Press *Revoke* on an access card to take back a folder before the session ends. Changing the reach to a narrower one takes effect from the next call. Ending the run ends the Autonomous grants.

## Hard limits no mode changes

- No shell. Imber cannot run an arbitrary command; processes start only through `run_start` (asks), `cargo_run` (asks once per target), recipes and scenarios (ask), the programs you set up in Settings (each with the arguments and variables you allowed), or the project's own build and test commands. Neither `cargo_run` nor `run_tool` goes through a shell.
- Secret files (`.env`, keys) are never written; reading one asks every time.
- `.git` is never changed by file tools.
- Deleting the project folder itself, an allowed folder, or anything outside the reach always asks.
- Push does not exist; a commit always asks.
- Spending limits apply in every mode (Imber › Cost and context).

---

# Imber: Reviewing changes

Whatever Imber changes can be read first, written all-or-nothing, undone afterwards, and recovered from Local History; risky work can happen in an isolated copy that touches nothing until you apply it.

## Proposals

In Manual mode a change is a **proposal**: a card in the thread listing the files with their added and removed lines. Nothing is written yet. A change made by one tool call (an edit across files, a rename, a replace over a folder) is one proposal, written all or nothing.

**Review** — on the card, or any file row of it — opens the proposal as a syntax-highlighted diff over most of the window, unified or side by side (the switch is in the header), with the files on the left, each with its folder and a bar of what it adds and removes (`↑` / `↓` or `J` / `K` move between them). The Review button itself shows the size of the change. Decide with **Apply all** (`Ctrl+Enter`) or **Reject all**. A rejected card turns red and keeps its files readable; an applied one stays as a record and can be undone. A proposal that touches a file of another project names it as `project:path`.

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

---

# Imber: Tools it can use

Imber acts only through a fixed set of tools, grouped in families; each family can be switched off in Settings, and a family that is off is not even offered to the model.

## The families

| Family (Settings label) | Tools | What it is for |
| --- | --- | --- |
| **Read and search files** | `search_text`, `find_files`, `structural_search`, `bennu_read_file`, `read_files`, `file_outline` | Read a file (or several in one call), see a file's declarations with their line ranges, search text, find files by name, and find or count code by its shape (`structural_search`: the same query language as the Structural search window — holes for statements, members, parameters and modifiers, `or`, `in`, `as`, `inside`, `group` — answering with counts, a sample of places or the `group` table; it never writes). |
| **Project intelligence** | `bennu_find_symbol`, `bennu_find_usages`, `bennu_references`, `bennu_callers`, `bennu_symbol_at`, `bennu_outline`, `bennu_check_file`, `bennu_problems`, `bennu_catalog`, `bennu_frameworks`, `bennu_module_graph`, `bennu_project_summary`, `bennu_tests`, `bennu_todos` and more | Bennu's own knowledge of the project: symbols, usages, beans, endpoints, framework catalogues, problems, tests, TODOs, the module graph, the profiler summary. A question about a position (symbol, declaration, references, implementations, rename, problems) is answered by the one engine that serves the file — the editor's own: a language server for its languages, the Java index, the Patina package for `.ptn`, the shader engine for `.wgsl` without `wgsl-analyzer`. When nothing serves a file the answer says why (no server for the extension, the server not installed, no project of its kind above the file) instead of reporting nothing found. While a language server is still loading, its answers say so, and `bennu_wait_ready` (given a file of that language) starts the server if nobody has and waits until it can answer — 60 seconds by default, at most 300 — instead of asking again and again. Engines do not all answer the same questions — a Lua server has no implementations or callers, the shader engine no rename, a Cargo manifest only completion and problems — so a tool asked something its engine does not answer refuses it, saying what that engine does answer; and `bennu_project_summary` lists, per language of the project, which questions its engine answers (a language server's own list once it is running). |
| **Framework model** | `bennu_endpoints`, `bennu_beans`, `bennu_entities`, `bennu_scheduled_jobs`, `bennu_config_properties`, `bennu_dependency_conflicts` | Typed, bounded lists built from the framework models: HTTP endpoints (Spring MVC / WebFlux, JAX-RS, servlets, Struts: method, path, handler, parameters, produces / consumes), DI beans (scope, profile, conditions, injection points and who injects them), JPA entities with relations and repositories, `@Scheduled` jobs, configuration keys and who reads them, and dependency version conflicts. Filter with `name` / `path` / `type` and a `limit`; a cut list says how many rows were left out. `bennu_dependency_conflicts` runs the build tool and is in the *Run* family. |
| **Edit files** | `edit_file`, `edit_files`, `replace_in_files`, `create_file`, `delete_file` | Exact edits (one or a list across files, all or nothing), text or regular-expression replace over a folder with a cap on files and a dry run that shows every match with its line and what it would become, create, delete. Arguments that arrive broken are not run: Imber is told where they went wrong and sends them again. |
| **Refactoring** | `rename_symbol`, `add_parameter` | Rename a symbol or add a parameter across every file that mentions it, from the project index, as one change. A renamed public type's file is renamed with it. |
| **Structural search and replace** | `structural_replace` | Rewrite code by its shape, safer than a regular expression. (Searching by shape is `structural_search`, in the read family.) Both walk only the files Imber may reach: a folder outside its scope that you did not open for it, a secret file (`.env`, keys, certificates) and `.git` are skipped before they are read — they are in no count, table or sample — and for a rewrite so is anything it could not also change (a folder opened for reading only, or everything while editing is off). The answer says how many files were skipped, never which. |
| **Local History** | `local_history` | List a file's revisions, show a diff, list deleted files, restore one. |
| **Build, run and test** | `bennu_check`, `bennu_build`, `bennu_test_run`, `bennu_cargo_run`, `bennu_npm_run_script`, `run_tool`, `run_script`, run configurations, launching a program | Anything that executes the project, and the programs and scripts you let it run. `bennu_check` says whether the project compiles: `cargo check` on Cargo (with a `target`, for that triple: "does it compile for wasm?"), `mvn compile` on Maven. `bennu_build` (Cargo) builds the files themselves — a real `cargo build` of the library and binaries for the host or a `target` (`wasm32-wasip2`…), with a `profile` (`release`…) — and its answer lists the artifacts it produced with their paths: the `.wasm` to install, for example, which a program you set up can then copy. A conversation that builds in its own target directory is told the artifacts are there, not in `target/`. `run_tool` runs the programs you set up in Settings › Imber (see Imber › Running and observing). On Cargo, `bennu_test_run` takes the features to enable (`features`, `all_features`, `no_default_features`), a module prefix (`cli::`) or a crate's package name as well as test names, and the runner — `cargo` or `nextest`; left out, nextest when the workspace has a `.config/nextest.toml`. With no `root`, a `module` that only another project of the conversation has runs in that project (also for `bennu_check` and `bennu_build`); otherwise name the project as `root`. A module prefix (`cli::`) runs every test under it, in each crate that has that module; a name not found is looked for again after a fresh scan, and the error lists the modules that hold tests, by crate and binary. `ignored_only` runs only the `#[ignore]`d tests of the selection, and `filter` takes a nextest filter expression (`-E`) for a selection names cannot say. When no test ran, the answer carries the compiler's (or Maven's) errors. While the project does not compile, `bennu_check` lists only the errors and counts the warnings; with no error, warnings from crates outside the project are counted, not listed. See Imber › Permissions and modes for when each asks. |
| **Music (Merula)** | `merula_check`, `merula_render` | For `.merula` files, through Merula itself. `merula_check` (in the *Project* family) evaluates a file as Merula's editor does, without playing it, and lists every error and warning with its line and column — syntax, evaluation, instruments or samples the sound registry does not know, speech controls on a non-speech source — with the number of tracks and the length it renders. `merula_render` (in the *Run* family) checks the file, then renders it to OGG (or WAV) next to it or at `out`, and answers with the file, its length and size, or with the errors when there are any. The length is the file's `meta { cycles = "N" }`, else its arrangement's, else one cycle, unless `cycles` says otherwise. A render asks as the project's build does, by the mode, and shows in Merula's Downloads & Exports with its Stop; stopping the run stops it too. Imber cannot hear the result: you do. |
| **Live services** | `run_start`, `cargo_run`, `run_status`, `run_logs`, `run_stop`, `wait_ready`, `list_recipes`, `run_recipe`, `http_request`, `trace_request`, `timeline`, `list_scenarios`, `run_scenario`, `save_scenario`, `console_logs`, `tail_file`, `wait_log`, `log_facets`, `log_record` | Start the project's services (a Rust binary or example with its features and arguments through `cargo_run`), read their logs, call them over localhost, follow one request through every log, run scenarios. See Imber › Running and observing. |
| **Debugger** | `debug_set_breakpoint`, `debug_list_breakpoints`, `debug_remove_breakpoint`, `debug_launch`, `debug_attach`, `debug_wait_stop`, `debug_stack`, `debug_variables`, `debug_evaluate`, `debug_step`, `debug_resume`, `debug_detach`, `debug_output`, `debug_run_to`, `debug_break_on_exception`, `debug_select_thread` | Debug a Java program. See Imber › Debugging with Imber. |
| **Look into a running Bevy game** | `brp_connect`, `brp_query`, `brp_get`, `brp_resources`, `brp_get_resource`, `brp_types`, `brp_watch`, `brp_screenshot`, `brp_stats`, `brp_actions`, `brp_states` | Read entities, components, resources and types over the Bevy Remote Protocol. Only for projects with a `Cargo.toml`. See Imber › Bevy games and the profiler. |
| **Screenshots of a page** | `screenshot_page`, `page_diagnostics` | Look at a front end on localhost. See Imber › Screenshots and diagrams. |

## Switches that are not in the family list

- **Let it change a running Bevy game** (`brp_mutate`, `brp_insert`, `brp_remove`, `brp_spawn`, `brp_despawn`, `brp_set_resource`, `brp_reparent`, `brp_call`, `brp_key`, `brp_type`, `brp_mouse`, `brp_gamepad`, `brp_action`, `brp_set_state`): off by default, needs the Bevy family on.
- **Let it change the program it debugs** (`debug_throw`, `debug_force_return`, `debug_drop_frame`, `debug_set_value`): off by default, needs the Debugger family on.
- **Git** (`corvus_status`, `corvus_diff`, `corvus_log`, `corvus_blame`, `corvus_stage`, `corvus_unstage`, `corvus_commit`): off by default. See below.
- **Sub-agents** (`spawn_agent`, `resume_agent`, `await_agents`): off by default; and the peer tools `file_owner` and `request_change` for talking to other conversations about a file. See Imber › Sub-agents.
- **Step plan** (`update_plan`): on by default.

## Tools that are always there

`ask_user` (questions, Imber › Decisions and questions), `bennu_help` (the help about Bennu itself), `open_ui` (opens a panel or window), `show_diagram` (draws a Mermaid diagram), `show_image` (shows a picture or an SVG, Imber › Screenshots and diagrams), `view_image` (Imber looks at an image file itself — a PNG, JPEG, GIF or WebP a program, script or test wrote — received like a screenshot, up to 3.5 MB; only the latest two pictures stay in what it reads), `load_skill` (only when you enabled a skill, Imber › Skills) and `expand_result` (returns the detail of a digest). In a chat only `bennu_help`, `open_ui`, `show_diagram` and `show_image` are offered.

## Switching a family off

Settings › Imber › Tools it may use has one switch per family. A family that is off is not offered at all: Imber cannot call it, its definitions cost nothing, and a call that slips through is refused with "the user has switched that kind of tool off". "Do not touch my files" is *Edit files* off; "do not run anything" is *Build, run and test* and *Live services* off. The switches are the defaults for new conversations and for the next message of the open one.

## Help about Bennu

Ask how to do something in Bennu (a feature, a shortcut, where a panel is) and Imber looks it up in Bennu's own documentation, this manual, and in a map of the window's buttons and shortcuts, instead of answering from memory. If the documentation has nothing, it says so. It works in a chat too, and so does opening a panel: ask to see the dependencies, the problems or the tests and Imber opens them; in a chat that needs no access to your project.

## Git

Off by default; Settings › Imber › Git turns it on. Imber can then read the repository (status, diff, log, blame) and stage, unstage and commit through Corvus's own git tools. A commit always asks first, in every mode, and it cannot push. Git is not available in a chat.

Reading means commit messages and author names go to the provider. **Hide authors** (on by default) shows the model each person as a made-up id such as `Author-7f3a9c`: the same person is always the same id, so "who wrote most of this" still works. The ids are turned back into names in what you read and in anything Imber sends to git, so a commit message crediting an id credits the real person. The mapping stays on this machine. Git also enables the "Git worktree" kind of isolated copy.

## Several projects

With the reach set to *All open projects* Imber works across the projects open in the workspace, the one you are in first, up to *Projects at once* (6 by default; each is a line in its instructions, so a higher number costs a little on every request). It reads, searches and changes in all of them: a tool takes a project by name (or `name:path`), the text search and the file finder take `project: "all"`, and a change goes to the project that owns the file, which keeps its own Local History. A proposal names a file in another project as `name:path`.

### Bringing in another project

Bennu's tools answer only for the projects of the conversation. When Imber needs them on another one, it names that project's root — usually with `bennu_project_summary`, which also says how far the index is — and the project is brought into the conversation before the call runs. Its instructions list the other projects you have open in Bennu, so it knows which are there. What happens depends on two things:

| The project | Imber may already read there | Imber may not read there |
| --- | --- | --- |
| **Open in Bennu** (already indexed) | It joins at once, without a question | The access card asks; a yes lets it join |
| **Not open in Bennu** (any folder elsewhere) | As *Opening other projects* says (Settings › Imber › Access): ask, except in Autonomous (the default); always ask; open without asking; never | The access card asks, and says that a yes also opens and indexes it — the only question |

- An opened project is indexed in the background, as an AI client's would be: until the index is ready, Bennu answers "not yet" rather than "nothing". It does not appear among your tabs, and it is closed after the idle time like any project nobody is looking at.
- A project brought in stays part of the conversation for its later runs (while Arbor is open) and counts toward *Projects at once*; past that limit it is refused with the reason.
- The thread says when a project joined or was opened.
- Any language Bennu has a language server for works: opening a project starts every server whose root marker is at its root (`Cargo.toml`, `package.json`, `.luarc.json`, `go.mod`, `pyproject.toml`, `angular.json`, `svelte.config.js`, a server of your own…), and Imber's symbol, outline, references, callers, implementors, rename and problems tools answer from it. The framework model (endpoints, beans, entities) is Java's. See Language servers.
- Any folder can be brought in, but only what can be indexed is: a Maven or Gradle build gets the Java index, and each language server whose root marker is there is started. A folder with neither joins as files.
- Never the root of a disk or your home folder itself. Never from a scheduled run, a sub-agent (it works in its parent's projects) or an isolated copy (it sees only the copy). With *Never*, a project not open in Bennu is only files to Imber.

## Big answers are digests

A build with sixty errors, a test run with forty failures, a long search or a long list of usages is answered with a digest (counts, distinct problems grouped, the files most affected and what was left out); `expand_result` returns the detail for one file, test or message. A file Imber already read comes back as one line when unchanged, and as the lines that differ when it changed.

---

# Imber: Running and observing

With the Live services family on, Imber can start the project's services, wait for them, read their logs economically, call them over localhost and run scenarios; builds from every conversation share one queue.

The feature itself (processes, recipes, scenarios, the timeline, correlation ids) is described on the **Live services** page. This page is about how Imber uses it.

## Managed processes

- `run_start` starts a named process with a command, a working folder, optional ports and a readiness check (a port, a log line or a health URL). It asks every time, showing the command. The line runs in `bash` where the machine has it (`sh` otherwise), and its `cwd` may be any project of the conversation, by name (`voriz:`, `voriz:crates/app`) — the same for `cargo_run`, whose `cwd` also decides which Cargo workspace it runs in, and for `run_tool`. Tools such as `cargo` and `mvn` are found even when Arbor was started from the Finder: at start it adds the usual tool folders (`~/.cargo/bin`, Homebrew, SDKMAN) to the `PATH` every process inherits. Imber is told to prefer Bennu's own tools for a check, a build, the tests or `cargo run` (`bennu_check`, `bennu_build`, `bennu_test_run`, `cargo_run`), which need no card and answer with the parsed result; a command stays available when one of them cannot do the job.
- `cargo_run` runs a binary or an example of a Rust project — package, `bin` or `example`, features, `release` or a profile, the program's own arguments and environment variables — as a managed process like any other. Imber gives fields, never a command line, and the command is the one a run configuration with the same fields would run, without a shell. The first run of a target (package and binary) in a conversation asks; the yes covers that target again with any features, arguments or environment, and another target asks again. A compile error shows in its logs. A server returns at once (its output is read with `run_logs`, also after it ends); a program that does its work and ends is run with `until_exit`, which waits for it (5 minutes by default, at most 30) and answers with its exit code and the last lines it printed, stderr marked — build errors included.
- `run_status`, `run_logs`, `wait_ready` and `run_stop` inspect, wait for and stop what the conversation started. Reading never asks; stopping what the run started does not ask.
- The processes are the same ones the Live panel lists. Every process records its owner; a conversation stops only its own processes. A name or port that another conversation holds is refused instead of duplicated or killed, and a stack another conversation already runs is attached read-only.
- A yes given to a recipes file in one conversation does not carry to another.

## Programs you set up

`run_tool` runs one of the programs listed in Settings › Imber › Tools and skills › *Programs it may run*, by name. Each declaration sets the program (an absolute path), the arguments always passed, whether Imber may add its own, the variables always set and the ones Imber may set, a time limit, whether it changes things, and when a call asks: every call, once per conversation, or never. A tool that changes things asks at every call in Manual mode, whatever it says.

- The program runs directly, without a shell: each argument reaches it as written, so `;`, `|`, `$(…)` and wildcards are plain characters. An argument or a variable the declaration does not allow is refused before anything starts.
- It runs in a folder of the project (its root unless Imber names one) and Imber gets back the exit code with standard output and standard error; a long output keeps its beginning and its end. Imber can also send text to its standard input.
- A call that reaches its time limit, or a run you cancel, ends the program with its whole process tree.
- With `background` the program starts as a managed process instead — for a watcher or a server — and is read and stopped like any other (`run_logs`, `run_stop`).
- A yes given once per conversation lasts until you edit the tool. A scheduled run never uses these programs.
- The settings page marks a tool that would not be offered (no program at that path, a name that is not allowed, no description) and says why.

## Scripts of the project

`run_script` runs a script of the project that is not one of your programs — `.sh`, `.bash`, `.py`, `.js`, `.ps1`, `.bat`/`.cmd` on Windows, or an executable file. It always asks first, with a card that shows the script's path, the command, why Imber wants to run it and its first lines. Three answers:

- **Decline** — nothing runs, and Imber is told.
- **Run once** — this call only.
- **Run and add to my programs** — it runs, and a declaration is added to Settings › Imber › Tools and skills › *Programs it may run* (interpreter, script, "changes things", asked once per conversation), for you to give a description and refine. From then on Imber calls it with `run_tool`.

The script runs through its interpreter, chosen by its extension, or directly when it is executable; arguments are passed one by one, with no shell. It runs like a program you set up: the exit code and the output come back, it is stopped after ten minutes, and `background` makes it a managed process. Imber can edit the project's files, so a yes is for the script *as it is*: a script that changed asks again, and the card says it changed. The same holds for a program you set up whose file is inside the project — or a script you handed to an interpreter in its fixed arguments: its approval covers that content, and even one set to *Never* asks again when the file has changed since it last ran in the conversation.

## Recipes and scenarios

`list_recipes` and `run_recipe` use the project's recipes file (`.arbor/bennu/live/recipes.toml`); `list_scenarios`, `run_scenario` and `save_scenario` use scenario files. Running a recipe or a scenario asks once per *content* of its file: change the file and it asks again. A scenario is an ordered list of steps with assertions and also runs without Imber from the Live panel.

## Requests and the timeline

`http_request` reaches services on this machine and nothing else (`localhost`, `127.0.0.1`, `[::1]`). GET, HEAD and OPTIONS do not ask; any other method asks unless the port belongs to a process the conversation started. `trace_request` follows one request through the logs of every process by its correlation id, and `timeline` reads the unified timeline of requests, log events and process events.

## Reading logs without reading them

A log is read as *records* (a line with its stack trace, the `Caused by` chain or a backtrace), digest first, detail only when needed:

- **`log_facets`**: a compact digest instead of lines, for a process, the Run console or a log file: exceptions grouped by class and root cause, queries with literals removed, requests, warnings.
- **`run_logs`**: a process's records with filters (level, pattern, time range, context) and a summary of the distinct errors.
- **`log_record`**: one record in full, by the id a digest printed.
- **`wait_log`**: waits for a new line matching a regular expression (default 30 s, at most 300), with context on each side.
- **`console_logs`**: what the Run console printed for the Run button (the last six runs of a project).
- **`tail_file`**: the end of a log file, whole records only; it returns an offset to read only what was appended next time. A read of a file follows the access rules.

Every answer lists what it left out and how to ask for it. Details of the filters are on the Live services page.

## The live run card

A run that used these tools ends with a **Live run** card: what it started and stopped, what is still running, the requests it sent (each opens the timeline on that request), the scenarios and assertions, the files it changed and what the run cost.

## The build queue

Builds and test runs take a numbered ticket in a queue shared by every conversation and by the Build button, and are served in order, one at a time per project. A waiting call simply returns its result when its turn has come; cancelling the run cancels the wait. Requests for the same module (or the same test selection) queued before a build starts are served by that one build, each with the same report and a note saying which state of the project it is about. The queue is listed in the Live panel.

**Build isolation** (composer settings menu, fixed when the conversation starts): *Shared (queue)* is the default; *Separate target* makes a Cargo project compile in a target folder of its own, so the conversation neither waits nor makes others wait, at the cost of disk space and a full first compile. Other toolchains stay in the queue. A conversation in an isolated copy never queues against the project.

## Who broke the build

When a build fails, the report says which errors are in files this conversation edited, which are in files another conversation edited last, and which other conversations changed anything since the project last compiled. It is a guess from the edit record, because in Rust an error often shows in one file and comes from a change in another. Bennu leaves a short system note for each of those conversations: it reaches Imber with its next tool result, or with your next message if it is idle, lists the errors without repeating itself and is marked as a heuristic. A file nobody edited in this session is reported as yours.

## Other conversations about a file

Whether sub-agents are on or not, Imber can ask who owns a file (`file_owner`: the conversation that reserved or last changed it, whether it is running, its current plan step) and leave that conversation a note (`request_change`). The note is marked as coming from another agent, not from you, is delivered once, and a run may send only a few.

---

# Imber: Debugging with Imber

With the Debugger family on, Imber can debug a Java program instead of guessing: it sets breakpoints and logpoints, runs the program under the debugger or attaches to one, waits for a stop, reads the stack and values and steps, on the very session the Debug panel shows.

The debugger itself is described on the **Debugging** and **Frames, values and watches** pages; this page covers what is specific to Imber.

## What it can do

| Step | Tools | Asks? |
| --- | --- | --- |
| Set, list, remove breakpoints and logpoints | `debug_set_breakpoint`, `debug_list_breakpoints`, `debug_remove_breakpoint` | No |
| Start a session | `debug_launch` (builds and runs a main class), `debug_attach` (to a JVM on 127.0.0.1) | Every time |
| Wait and look | `debug_wait_stop`, `debug_stack`, `debug_variables`, `debug_evaluate`, `debug_output` | No |
| Move | `debug_step` (several steps at once), `debug_resume`, `debug_run_to` (like Run to Cursor), `debug_break_on_exception`, `debug_select_thread` | No |
| Let go | `debug_detach` | No |

## Its breakpoints are visible and yours

Breakpoints Imber sets are saved in the project's breakpoints, drawn with a sparkle in the gutter and badged *Imber* in the Breakpoints list (`Ctrl+Shift+F8`). You can disable or delete them like any other. It never changes or removes one you set, and holds at most 20. A logpoint can say "only when `total > 100`" through a condition and a pass count, and can ask for values to be captured at the stop. A line that holds no code (a comment, a brace, an annotation, a declaration) is moved to the next line that does, and Imber is told which line it ended on.

## Reading values cannot change anything

Values are read as *paths* (`order.customer.name`, `items[2].price`) through fields and array slots. Method calls, constructors, assignments and `++` are refused before they reach the program, with or without approval. Variables named like secrets (password, token, secret, apiKey, credential…) are masked. Results are capped: frames, variables and long values are cut and say so.

Nothing is ever run in the program, so `toString()` is never called. Instead, the common JDK types are read from their fields: a `BigDecimal` reads as `123.45`, dates and times (`LocalDate`, `OffsetDateTime`, `Instant`, `java.util.Date`…) as ISO text, enums as their name, boxes and `Optional` as their content, and lists, maps and sets as `ArrayList(size=3)`. Any other object reads as `Type@id` and is opened with a longer path.

## Several threads, and why a stop happened

Each stop says whether it is *new* (a breakpoint reached now), *a step*, or *leftover* (hit while another stop was still open), and which condition let it through with the values it read. A breakpoint Imber sets with `suspend: "thread"` stops only its own thread; when several threads are stopped the report lists the others, `debug_select_thread` switches between them, and `debug_step` and `debug_resume` accept a `thread`. Resuming moves on to the next waiting stop. `debug_step` skips a record's accessors, and with `return_value` it also reports what the method it left returned. Removing a breakpoint resumes the threads parked on it, never the stop in front of you.

## Reading the program's output

Each launch keeps what the program prints under its session id, and `debug_output` reads it also after the program has exited. Without an id it reads the program *this conversation* launched last, so another conversation starting its own debug run never changes what Imber reads; the id that `debug_launch` printed selects a specific one. The last eight launches are kept.

## The session belongs to the conversation

The session outlives one message: when a run ends the program stays alive (suspended, if it was stopped at a breakpoint, so you can inspect it) and the next message goes on from there. There is one debug session per conversation. It ends when Imber detaches (a program it launched is stopped, one it attached to is resumed and left running), when the program exits, after the idle time in Settings › Imber › Debug session idle time (30 minutes by default, counted from the last debug tool), when the project closes, or when you press *Stop* beside the debugger controls, where the line "Imber … ends in N min if unused" shows how long is left. Breakpoints it set without starting a session stay for you to use.

## When the program stops while Imber is not waiting

Bennu itself notices the stop and takes a snapshot while the program is still suspended: where it stopped, the top of the stack, the top frame's locals and the values the breakpoint asked to capture (capped and with secrets masked). What happens next follows the conversation:

- **A run is active**: the snapshot reaches Imber as a system note with its next tool result.
- **No run is active**: the note is queued for your next message, and the thread shows a card ("Stopped at Order.java:42 in OrderService.total") with *Send to Imber now*.
- **Start a run by itself**: off by default (Settings › Imber › Start a run when the program stops, or per conversation from the composer's menu › Decisions). At most three an hour per conversation. It spends money and counts toward your limits.

## Changing the program: a separate switch

*Let it change the program it debugs* (off by default) offers four tools: `debug_throw` creates an exception inside the program (its constructor runs there) and throws it from the line the stopped thread is on, `debug_force_return` makes the method on top of the stack return a literal value at once (finally blocks included), `debug_drop_frame` discards a frame so the call runs again, `debug_set_value` writes a number, boolean, character, String or null to a local, a field or a path such as `order.total` (final and static fields are refused). All need a stop; the last two need a VM that supports them. The program changes while it runs and **there is no undo**: side effects already produced stay. Imber asks once per conversation (Autonomous does not skip the question) and every change is listed in the thread and in the Debug panel as an *Imber action*.

The debugger tools target Java programs: a main class to launch, or a JVM listening on a local debug port.

---

# Imber: Bevy games and the profiler

Imber can look into a running Bevy game over its Remote Protocol, change it only if you allow that, and read the newest profiler recording to propose a performance change.

## Looking into a running game

The *Look into a running Bevy game* family (Settings › Imber › Tools it may use, for projects with a `Cargo.toml`) connects to a game over the Bevy Remote Protocol. The game needs `RemotePlugin` and `RemoteHttpPlugin`, and the connection is to this machine only.

- `brp_connect`: connects, and waits for the game to come up if it is still starting.
- `brp_query`: entities with their component values.
- `brp_get`: one entity in full.
- `brp_resources` and `brp_get_resource`: the resources and one resource's value.
- `brp_types`: the type registry, so it knows the shape of a component before it asks for it.
- `brp_screenshot`, `brp_stats`, `brp_actions` and `brp_states`: the game's window as an image (900 px wide unless asked otherwise), its frame time and diagnostics, and the actions it lets a remote client run. These need the `arbor-bevy-remote` plugin in the game (see **Bevy Remote**).
- `brp_watch`: one entity observed for a few seconds (5 by default, 30 at most), reporting which fields changed and their first and last value.

Reading never asks. The connection itself and the protocol are explained on the **Bevy Remote** page.

## Changing a running game

*Let it change a running Bevy game* in Settings is a separate switch, off by default, and it needs the Bevy family on. It adds `brp_mutate`, `brp_insert`, `brp_remove`, `brp_spawn`, `brp_despawn`, `brp_set_resource`, `brp_reparent` and `brp_call`, and, to play the game, `brp_key`, `brp_type`, `brp_mouse`, `brp_gamepad`, `brp_action` and `brp_set_state` (`brp_action` runs one of the game's own registered actions). `brp_call` calls any Remote Protocol method by name; a method that only reads (`rpc.discover`, anything named `query`, `get` or `list`) runs without the question, any other asks like the rest. These act on live state with no review and no undo. Imber asks once per conversation ("Let Imber change the running game?"), the yes covers every change in that conversation, and every change is listed in the run's summary. Autonomous mode does not skip the question.

## Reading a profiler recording

The project-intelligence family includes three read-only profiler tools. `bennu_profile_recordings` lists the recordings the Profiler tool window made. `bennu_profile_diff` compares two of them method by method (the share of the run each took before and after, biggest changes first), to check that a change made the code faster. `bennu_profile_summary` is a compact summary of one recording (the newest unless Imber names another): the hottest methods with their source lines, the biggest allocation sites and allocated types, lock contention and garbage-collection pauses. Imber uses it to find the method worth optimising and to propose a change. These three only read: with no recording they tell you to make one first. Recording by hand is described on the **Profiler** page; Imber recording for you is below.

## Recording a running JVM

The **Profile** family (`profile_start`, `profile_stop`) lets Imber record a JVM that is already running on this machine with Flight Recorder, through `jcmd`. **Imber asks at every start**, naming the JVM, because a recording samples a running program (nothing in it is changed, it runs a little slower). Imber names the JVM by process id or by part of its main class; with neither, it gets the list to choose from. One recording per conversation, started in one message and stopped in a later one. `profile_stop` ends the recording, waits until it has been read and saved, and returns its summary; the recording is then in the Profiler window like any other. A recording keeps sampling until it is stopped. Sub-agents never have these tools, and the family can be switched off like any other.

---

# Imber: Screenshots and diagrams

Imber can look at a front end running on localhost, draws Mermaid diagrams and shows pictures in the thread when a picture explains better than text, and you can attach pictures and files to what you send it.

## Screenshots of a page

The *Screenshots of a page* family lets Imber see a page served from `localhost`, `127.0.0.1` or `[::1]`, and nothing else. `screenshot_page` takes a URL, or a port of a process it started plus a path, and can wait for a selector or a number of milliseconds. `page_diagnostics` reports the console, failed requests and the page text.

- **With Playwright in the project**: the picture plus console errors, failed requests and the page text; it can capture one element or the full page.
- **Without Playwright**: an installed Chrome, Edge, Brave or Chromium gives the picture only.
- A screenshot appears in the thread as a small preview and enlarges on click or Enter. The model keeps only the latest two, and they count in the context bar. A picture over about 3.5 MB is refused.
- No file is read or written and nothing asks; the restriction to this machine is in the tool.

Pictures are expensive in tokens, which is why the family can be switched off on its own.

## Diagrams

When a picture explains better than text (an architecture, a flow, a call chain, the entities of a model) Imber draws a **Mermaid** diagram (flowchart, sequence, class, state, ER, Gantt…) with `show_diagram`, in a chat as well as in an agent conversation.

- Each diagram is checked before it is shown. If the source does not parse, Imber is told the error with its line and sends a corrected one, up to three tries; after that the source is shown as text with a notice. The failed attempts are dropped from what is sent to the model afterwards, so they cost nothing later.
- A diagram follows the theme, is capped to a readable height and scrolls inside its box. Its buttons copy the Mermaid source and open it larger in a window.
- A `mermaid` code block in any rendered answer, or in an answer opened as a document, is drawn the same way.
- The source is capped at 20,000 characters.

## Pictures in the thread

`show_image` puts a picture in the thread: an image file of the project (PNG, JPEG, GIF, WebP or SVG) or an SVG Imber wrote itself, such as a logo or an icon. It shows as a small preview with a caption; click it, or focus it and press Enter, to open it large. When a message holds several pictures the **←** and **→** keys walk them and **Esc** closes. In the large view a click on the picture zooms to 2× at that point and a second click fits it again; `+` and `−` zoom in steps up to 8× (`0` fits), `Ctrl`/`⌘` + wheel or a pinch zooms at the pointer, and a zoomed picture is moved by dragging or scrolling.

- The picture is for you: Imber does not get its pixels back. To have it *look* at an image, attach the image.
- A file is read like any other (the reach and the Read family apply, and a file outside the reach asks); a chat has no files, so it shows SVG it wrote. A file over 5 MB is not shown.
- An SVG is drawn as an image, so nothing inside it can run.
- The pictures are kept beside the conversation and go with it when it is deleted.

## Attaching files

The **paperclip** in the composer (or the command *Imber: Attach files to the message…*) opens Bennu's file explorer to pick files; you can also paste a picture from the clipboard, or **drag files** from the desktop onto the composer (a dropped file is sent as its contents, so it must be under 4 MB). Attachments appear as chips above the text, each with an ✕; **Backspace** in an empty field removes the last one. They go with the next message and appear above it in the thread.

- **Pictures** (PNG, JPEG, GIF, WebP) are sent to the model as images, up to 3.5 MB each. Like screenshots they cost tokens and count in the context bar; the model keeps seeing the latest four you attached and older ones become a line.
- **SVG** is shown as a picture to you and sent as its source text: the model cannot read the image itself.
- **Text and code** files are sent inline, named, up to 200 KB each and 500 KB per message. A file that is not UTF-8 is decoded with the project's encoding.
- **Anything else** (PDF, archives, Office documents, binaries) is refused with a reason before anything is sent, and your text and files stay where they are.
- Up to eight files per message. Choosing a file is your consent: an attachment is read wherever it is, whatever the reach is set to.

---

# Imber: Skills

A skill is a folder with a `SKILL.md` that teaches Imber how you want one kind of task done; none is known to Imber until you enable it in the skills manager.

## The format

The format is the one Claude Code uses: a `SKILL.md` with a frontmatter holding `name` and `description`, then the instructions. Examples: a release checklist, how to write a migration, how your team names tests.

```
---
name: write-migration
description: How to write a database migration in this project
---
1. Put the file in db/migrations, named V<next number>__<what>.sql.
2. Never edit a migration that has been applied.
```

## Where skills are looked for

- each project's `.bennu/skills` and `.claude/skills`;
- Bennu's own `skills` folder in its config folder;
- the folders you add under **Settings › Imber › Tools and skills › More folders of skills**, in the order you added them — a folder that holds skill folders, or one that is a skill itself (it has a `SKILL.md`);
- `~/.claude/skills`.

A project skill hides a user skill of the same name, and `.bennu` wins over `.claude`. Among your own folders the list above is the order: a skill in a folder you added wins over one of the same name in `~/.claude/skills`. Skills from an added folder are user skills, labelled with the folder's name.

## The skills manager

Open it from **Skills…** in the panel's **…** menu, the composer's settings menu, Settings › Imber › Skills, or the palette entry *Imber: Manage skills…*. It lists each skill with where it came from, its size and a preview of its text; switches them one by one or all at once; and lists folders it could not use with the reason. The Settings page and the composer show "N of M enabled".

Enabling is an allow-list stored in the config (`skills_on`, ids of the form `user/name` or `project/name`): a skill found on disk but not ticked is invisible to Imber.

## How a skill is used

- Only a one-line index of the enabled skills (name and description) travels with each request.
- The folder of an enabled skill is readable by Imber whatever the reach — its `SKILL.md` and the files it points at (references, examples) — without an access card; writing there follows the ordinary rules.
- Imber loads a skill's text with `load_skill` when a task matches, or when you start your message with `/name`. Type `/` in the composer to pick from the enabled skills.
- Limits: a skill's text is cut at 32 KB, at most 50 skills are read, and a description is cut at 240 characters.
- Nothing in a skill is ever run. Other files in its folder are only named; Imber can read them with its ordinary tools only if the reach allows it.

Skills are not available in a chat.

---

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

- The *Agents* button in the tray over the message box says how many are running; it opens the list with each agent's state, how long it has run, what it cost, and its Stop.
- The title opens its thread, read-only and live (*Imber: Open the latest sub-agent's thread…*). It reads as a chat between the two: the task Imber (badged *main*) gave it, then the agent's own messages (badged *sub-agent*), a writing mark while it works and, at the end, a line saying it handed its summary back, stopped or failed. The view follows the end of the thread unless you scrolled up.
- Each agent has a Stop; *Stop all* and *Imber: Stop all sub-agents* stop them together.
- A card for each stays in the thread: what it was asked, what it may do, its state, time and cost, and the start of its answer. While the agent runs the card's border moves. An agent's record and thread are kept on disk for as long as its conversation keeps them, so the Agents list and an agent's thread still open after a restart; an agent that was running when the app closed shows as stopped. What it read is not copied: the kept thread says *read this file*, not the file. Imber can continue a finished agent with `resume_agent` and a new instruction: it remembers what it concluded and reads again what it needs. The trash button on a finished agent deletes it; compacting the conversation, or deleting it, deletes them all.

## A hint, not a decision

When the window is filling up (from 70% at medium effort, 50% at high; never at low) and your new message looks unrelated to the files worked on so far, a one-line note is added at the end of that message suggesting an agent. Imber decides.

---

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

The same window also holds the conversation's own switch for *Start a run when the program stops* (see Imber › Debugging with Imber).

## Questions

Where it would otherwise guess, Imber asks with `ask_user`. Each question is a column of choices, one per row: pick one, or tick several, and *Other…* is always there to type your own. When it asks several things at once, a strip of tabs names them (a tick once answered): jump to any, or use **Back** and **Next**; the last step sends all the answers together, so changing an earlier one costs nothing. Answered, the card shrinks to what you decided and stays in the thread.

Questions about access or confirmation ("Allow this?", "Start a process?") are not part of this: they follow the mode and reach, see Imber › Permissions and modes. Sub-agents cannot ask you questions.

---

# Imber: Schedules

A schedule asks Imber something at a time you choose: once, every N minutes, every day, on chosen weekdays, or on a cron expression. "Every weekday at 09:00, tell me what changed on main" is one schedule.

## Creating one

- **By hand.** Open the window from **Schedules…** in the panel's **…** menu or the palette entry *Imber: Schedules…*, then **New** (or press `N` in the list).
- **From the composer.** Type the prompt, then run *Imber: Schedule this prompt…* from the palette: the form opens with the text already in it. The entry appears only when the composer has text and a project is open.
- **By asking Imber.** Say it in a conversation: "every weekday at 9, check what changed on main". Imber always shows the schedule in words and waits for your approval before it exists, whatever the access mode.

The form is short: name, when, what it does, what to do when the run was missed, and an on/off switch. The name takes the focus when it opens, `Tab` goes through the fields, `Ctrl+Enter` saves and `Esc` goes back.

## What a schedule does

| Action | What happens |
| --- | --- |
| **Wake this conversation** | The saved prompt is sent into the conversation the schedule was made from, which carries on as if you had written it. |
| **Ask Imber in a new conversation** | A fresh conversation each time, titled after the schedule. The outcome is shown on the schedule. |

Write the prompt so that it needs no answer: say what to check and what to report. Nobody is there to reply.

## A scheduled run is read-only

- The run reads and searches the project, and any change it wants to make is left as a proposal in its conversation, waiting for you. It never modifies the project, runs commands or asks for confirmation.
- If Imber asks a question, the run ends as *incomplete* instead of waiting.
- A scheduled run cannot create, change or delete schedules.
- A schedule Imber creates is no freer than the conversation that created it, and an MCP client can only create read-only ones.

## Only while Bennu is open

Schedules fire only while Bennu is running; there is no background service. When a run's time passed while Bennu was closed, the schedule decides: **Skip it** (the default) drops it, and the schedule shows it as *Skipped* rather than as a failure; **Run it once on the next start** catches up with a single run. A one-time schedule that was missed and skipped turns itself off.

## Cost limits

Scheduled runs spend from the same limits as the panel and are also capped on their own: a **scheduled run cap** (euros per run; the run ends as incomplete when it gets there) and a number of **scheduled runs per day**, so a mistaken schedule cannot spend the month overnight. Both are in Settings › Imber › Spending limits; 0 switches one off.

## Knowing it ran

A small toast says "Schedule … started" at every start, including **Run now**, and a failure shows its reason, once per episode rather than at every attempt. The list shows when each schedule runs next and how the last run ended.

## The window

| Key | Action |
| --- | --- |
| `↑` `↓` | Move between schedules |
| `Enter` | Edit the selected schedule |
| `Space` | Turn it on or off |
| `R` | Run it now, outside its times (its next run is unchanged) |
| `Del` | Delete it, after a confirmation |
| `N` | New schedule |

A schedule that wakes a conversation also has a button that opens it.

## From other tools

AI clients connected over MCP get two tools: `bennu_imber_schedules` lists the schedules, and `bennu_imber_schedule_create` creates a read-only one. Neither can change or delete an existing schedule.

## Where they are saved

The list lives in Bennu's config folder, outside your projects, so a run's bookkeeping never shows up in version control. Each schedule belongs to the project that was open when it was made.

---

# Imber: Cost and context

Every request carries the whole conversation, so the cost depends less on how much Imber writes than on how much it re-reads; Bennu keeps that small, shows where tokens go, and stops a run at the limits you set.

## The context meter

Above the composer a bar shows how full the model's window is, split into *tool definitions*, *Imber rules*, *project instructions*, the *skills index*, *files and search results*, and the *conversation*. It stays calm until 75%, turns amber there and red past 90%. Open the line for the breakdown and for how much of the last request came from the provider's cache. The figures move while a run goes, with each request, and so does the conversation's cost under the composer. They are estimates by characters, corrected by what the provider counted: four characters to a token is the rule for English prose, and code, JSON and other languages pack fewer into a token — near half as many on a real conversation. After each request Imber measures how far its estimate was from the provider's count, keeps that ratio with the conversation, and corrects the meter, the moment compaction starts and the room left for the answer with it.

*Show what would be sent…* (the panel's **…** menu) lists the same parts for the next request, in the order the provider reads them, with the cost when the cache is cold and when it is warm.

## Models, prices and the context window

The model list and prices are a file, `models.toml`, built in. **Settings › Imber › Models and prices › Create a price file** creates your own copy (`assistant-models.toml` in Bennu's config folder): an entry with the same id replaces the built-in one and any other id is added, so a price change or a new model needs an edit and a *Reload*, not a new build. An entry gives `[model.price]` (`input`, `output` and optionally `cache_read`, `cache_write_5m`, `cache_write_1h`), the `context_max` window, the `max_output` one response may have (thinking included; 32,000 when left out), the `efforts` it accepts and, for a model that charges more for long prompts, `[model.long_prompt]` with `over_tokens` and its own rates. Cost is worked out request by request, with cache reads and writes at their own rates. Each request lets the answer be as long as the model allows — its `max_output`, less only when the conversation fills so much of the window that there is no room for it: output is paid for as it is written, so a high ceiling costs nothing by itself, and the spending limits are what stop a run.

The **context window** is the largest window Imber may use in a conversation, 50K up to 1M; a size the model cannot hold is listed but cannot be picked, and changing to a smaller model shrinks the window to fit.

## How the cost is kept down

- **The provider's cache.** Tool definitions, instructions and the conversation so far are cached, so a request pays full price only for what the previous one did not already send. Anything that changes the start of a request starts it over, which is why the open file travels with your message and not with the instructions.
- **Long cache.** By default the tools and instructions are kept in the one-hour cache (Settings › Imber › Keep instructions cached for an hour) instead of the five-minute one. It costs a little more the first time and pays off when you pause for more than five minutes between messages or open another conversation in the same project.
- **Reading the same thing twice.** A file already read comes back as one line when unchanged, and as the lines that differ when changed. `file_outline` and `read_files` help open big files by the part that matters.
- **Big results are digests.** Counts, grouped problems, the files most affected, and a statement of what was left out. `expand_result` gives the detail.
- **Old results are stubbed in a batch, and only when room runs out.** Once the conversation fills half the context window and many old file reads and search results have piled up — results of the request being worked on, and of files still being read or edited, stay whole — they are replaced all at once by one-line stubs saying how to fetch them again, and the thread says so. In a batch, never one per message, because changing a message makes the cache start over from it.

## Compacting

**Compact conversation…** (offered by the context meter) first clears the output of older tool calls, then folds everything before your last message into a summary. What you type in the dialog is what the summary must not lose, as with `/compact "…"` in Claude Code.

- The summary is written by the **cheaper side model** (Settings › Imber › Cheaper model for side jobs; by default the cheapest Haiku in the price list), with fixed sections: goal, decisions, state, open questions and your preferences, followed by the files changed and the current plan taken from the record word for word.
- It happens when you ask, automatically when the conversation reaches 90% of the largest window you chose (clearing first, summarising only if that is not enough), and at the end of a run once the conversation is past 60% of the window.
- The thread says how many messages became how many tokens and which tool output was not kept. Compacting costs one request, added to the conversation's cost.

## Changing model or window in a long conversation

A new model cannot reuse what the old one has cached, so a conversation of several thousand tokens sends all of it again at full price. Bennu asks first, with an estimate. A smaller context window asks only when the conversation already fills it (it would be compacted first); a larger one changes nothing. Turn the question off with **Settings › Imber › Confirm costly changes**.

## When a request fails

A failed request is told apart by what caused it, and only the causes that pass by themselves are tried again.

- **Sent again, automatically:** no connection or DNS failure, a connection that goes silent (no data for two minutes) or drops while the answer streams, rate limit (429), overloaded (529) and server errors (5xx).
- **Never retried:** a rejected key (401/403), no credit or an account usage limit, a conversation too long for the window, an unknown model, an invalid request, and your own Stop. These end the run at once with the reason and what to do.
- **How long it waits:** 1, 2, 4, 8, 16 and then 30 seconds, each spread a little at random so agents that fail together do not return together, never less than the provider's own `retry-after`. A network that stays down is waited out for about three minutes in total; a provider that keeps answering "not now" is asked five times. A `retry-after` longer than two minutes is reported rather than waited for.
- **While it waits** the thread shows one calm line with the attempt and the seconds left: **Retry now** skips the wait, **Stop** ends the run at once. There is no toast per attempt. When the connection returns the line goes and the answer continues; the text the broken attempt had already shown is taken back and written again.
- **Money:** an attempt that failed after the provider had started it can still have been billed; it is counted once, as a request of its own. Spending limits are checked before every retry exactly as before a first request.
- **When it gives up** the message says what happened and the next step; the provider's own words are under *Details*. Every turn that finished is saved. **Retry** under the message sends the same request again, with no new message from you.
- **Compaction** (manual, automatic and at the end of a run) waits only about twenty seconds, then stops with the reason; the conversation is left exactly as it was and the next run tries again.
- **Sub-agents** retry the same way; their card shows *waiting to retry*. A failed agent reports to the conversation what failed, whether starting it again can help, and whether it had already changed files.

## Spending limits

Settings › Imber › Spending limits holds limits in euros: **per conversation** (default €5), **per day** (from midnight, your time; €10), **per week** (from Monday; off), **per month** (from the first; €50). `0` switches one off.

- A conversation or a period at its limit does not start another request, and a run in progress stops before the request that would take it over; the thread says which limit.
- The composer flags a limit from 80% of it (**Warn from**, adjustable) and shows what the conversation has cost, updated with each request while a run goes.
- The title bar of Bennu and of Corvus has a credit meter, just left of the documentation button: Imber's spark in a ring. The ring fills with what the providers have left on average, and takes the colour of the worst one — amber when one is close to running out, red when one has. Pressed, it lists every provider (the active one first and open) with its figure; open a row for where the figure comes from. It reads in the background every few minutes and after each answer; **Refresh** asks again at once.
- For Anthropic, the API does not report the credit left on the account, so it is **estimated**. Enter the balance shown in the Claude Console under **Settings › Imber › Models and cost › Credit balance**; the meter shows *~$… left*: that balance, less what was spent after you entered it. Nothing spent before is subtracted, older conversations included. With an **Admin key** (Settings › Imber) what is subtracted is your organization's real spending from Anthropic's cost report (from the day after, plus Imber's own for the rest of the day you entered it) — close to the real figure. Without one, only what Imber spent — a rough estimate, since anything else billed to the key is not seen; the row says which. Enter the balance again after a top-up, or now and then. Only an Anthropic organization has Admin keys; an individual account cannot.
- With no balance entered, the figure is what is left of the tightest spending limit you set (or what this month has cost, with no limit).
- Limits count what conversations still running have spent so far, so two at once cannot together pass a daily, weekly or monthly limit that neither passes alone.
- Costs are recorded per run in a ledger of their own (`usage.jsonl` in the data folder): deleting a conversation does not hand its money back.
- Prices are in dollars, so amounts are converted at the **Exchange rate** you keep by hand (default 0.92 euros per dollar); with no usable rate a dollar counts as a euro. The rate never moves on its own.
- Sub-agents count against the same limits and have a cap of their own (Imber › Sub-agents).

---

# Imber: Working from the editor

Imber can be asked from the margin of the editor, from a doc comment, from a Markdown selection and from a stack trace, each with a ready-made request; pasted code is fenced automatically and the panel can take the room of the editor.

## The sparkle in the margin

A sparkle in the margin sits on the line the caret is on, or where a selection starts, and offers what fits what is there:

- on a line with a problem: *Fix this error* or *Explain this problem*;
- on a selection: *Explain*, *Look for bugs*, *Suggest improvements*, *Write tests*;
- on a declaration: *Explain*, *Write tests*, *Where is it used?*, *Write documentation*;
- always: *Ask about this…*, which leaves the start of a message in the composer.

The request carries the file, the lines and the code itself, and starts a conversation of its own. From the keyboard, *Ask Imber about this…* in the Command Palette does the same, opening the menu at the caret.

## Doc comments

In a Java or Rust file a sparkle in the gutter sits on every doc comment, and under the caret on a declaration that has none. It offers *Improve this comment*, *Check against the code* (parameters, return value, exceptions, statements the code contradicts), *Make it shorter*, *Translate to English / Italian* and *Write documentation*. The request carries the file, the lines and the declaration, so there is nothing to type; the result comes back as a proposal like any other edit.

## Markdown files

In a Markdown file, selecting a passage shows a small button at its end: *Improve the writing*, *Make it shorter*, *Say more*, *Fix spelling and grammar*, *Translate*.

## Stack traces

In the Run and Build output, the line that starts a stack trace has a *Fix with Imber* button that sends the trace and asks for the cause in the project.

## Pasting code into the composer

Code pasted into the message box becomes a fenced block with its language (`` ```java … ``` ``), with a blank line either side, at the caret; one `Ctrl+Z` takes it back out. A paste counts as code when it spans several lines and looks like source (brackets, statements, indentation, a recognisable language such as Java, Kotlin, Rust, TypeScript, JSON, XML, SQL, YAML, properties or shell); prose, lists and single lines are left as they are. The language comes from the text, or from the open file when the text does not say. A paste made inside a fence does not open another one.

- Each block of the message being written is listed above the box with its language, line count and first line. Click a row to read the block highlighted, or the cross to remove it.
- In the thread your message shows its blocks highlighted, like Imber's own.
- `Ctrl+Shift+V` pastes exactly what is on the clipboard, with no fence. **Settings › Imber › Wrap pasted code in a fenced block** turns the wrapping off.
- A paste over 200 KB is cut at a line and a notice says so.

## Expanding the panel

The button in the panel header (or a double-click on the header, `Ctrl+Shift+U`, or *Imber: Expand panel*) covers the editor area and the bottom dock; the title bar, the rails and the side panels stay. The same button, the same keys, *Imber: Restore panel* or `Esc` from inside the panel (when no menu or window is open over it) put it back at the width it had. The editors are only covered, not closed: their scroll, cursors and unsaved changes stay as they were. Expanded, the conversation is a centred column: prose keeps a comfortable line length, while code, diagrams, diffs and the cards for live runs use the full width. Closing the panel, or opening another one on the right, ends the expansion.

## What travels with a message

The file you are in and the lines you selected travel with the message, shown as chips above the text; the selection is read when you click into the composer. Instruction files in play appear as chips too.

---

# Imber: Settings reference

Every Imber setting, in the order of the page Settings › Imber: what it does, its default, the key it is stored under, and whether a conversation can override it.

## Where settings are stored

- All settings below are in the `[assistant]` table of Bennu's config file for the active profile, `arbor/profiles//bennu/config.toml` (the limits in `[assistant.limits]`). The key is the name in the "Key" column.
- The **API key** is never in a config file: it is in a credential vault of its own.
- The **price list** is `assistant-models.toml` and your **own instructions** are `assistant-instructions.md`, both in the same Bennu config folder. Your own skills go in its `skills` folder, or in any folder you add under Tools and skills.
- Conversations, the spending ledger (`usage.jsonl`) and the git author mapping live in the data folder `arbor/data/bennu/assistant`. Isolated copies live in `assistant-worktrees` beside it.

## Connection

| Setting | What it does | Default | Key | Per conversation |
| --- | --- | --- | --- | --- |
| **API key** | Your Anthropic key. Save, or Remove. | Not set | (vault) | No |
| **Admin key (optional)** | An Anthropic Admin key, read only by the credit meter in the title bar to show what your organization really spent this month; never sent with a conversation. Only Anthropic organizations have Admin keys. | Not set | (vault) | No |

## Models and prices

| Setting | What it does | Default | Key | Per conversation |
| --- | --- | --- | --- | --- |
| **Price list** | Create a price file, a copy you can edit to change a price or add a model; Reload applies it. The path is shown below the buttons. A file that cannot be read is reported and the built-in list is used. | Built-in list | (file) | No |

## Git

| Setting | What it does | Default | Key | Per conversation |
| --- | --- | --- | --- | --- |
| **Let Imber use git** | Status, diff, log, blame, stage, unstage and commit through Corvus. Not available in a chat. | Off | `git_enabled` | No |
| **Hide authors** | Shows the model each person as a made-up id; turned back into names for you. Needs git on. | On | `hide_authors` | No |

## Spending limits

| Setting | What it does | Default | Key | Per conversation |
| --- | --- | --- | --- | --- |
| **Per conversation** | The most one conversation may cost, in euros. 0 = off. | 5 | `limits.session_eur` | No |
| **Per day** | From midnight, your time. | 10 | `limits.day_eur` | No |
| **Per week** | From Monday. | 0 (off) | `limits.week_eur` | No |
| **Per month** | From the first. | 50 | `limits.month_eur` | No |
| **Exchange rate** | Euros per dollar, kept by hand. | 0.92 | `limits.eur_per_usd` | No |
| **Warn from** | The composer flags a limit at this percentage of it (10 to 99). | 80 | `limits.warn_percent` | No |
| **Scheduled run cap (EUR)** | The most one scheduled run may cost, in euros; it ends as incomplete when it gets there. 0 = no cap. | 1 | `limits.schedule_run_eur` | No |
| **Scheduled runs per day** | How many scheduled runs may start in a day. 0 = no cap. | 24 | `limits.schedule_runs_per_day` | No |
| **Credit balance** | The account's balance as the Claude Console shows it, in dollars, entered by hand (the API does not report it). Saving records the moment too: the title bar's meter estimates what is left from what is spent after it. Update, or Clear. | Not entered | `limits.credit_usd`, `limits.credit_set_at` | No |

Each limit field also shows how much has been spent against it.

## New conversations

| Setting | What it does | Default | Key | Per conversation |
| --- | --- | --- | --- | --- |
| **Model** | What a new conversation starts with. | Opus 5.5 | `model` | Yes, composer |
| **Largest context window** | The most Imber may put in one conversation. | 200K | `max_context` | Yes, composer |
| **Effort** | How hard it thinks (Low to Max); disabled for a model with no effort setting. | High | `effort` | Yes, composer |
| **Steps per run** | Requests to the model one run may make before it stops with “Stopped after N steps” and waits for you to say *continue* (5 to 300). A step is one request, however many tools it calls. Also applies to Imber in Corvus. | 40 | `max_steps` | No |
| **Cheaper model for side jobs** | Writes compaction summaries and runs read-only sub-agents. Empty means the cheapest Haiku in the price list. | Default (cheapest Haiku) | `side_model` | No |
| **Keep instructions cached for an hour** | Use the provider's one-hour cache for the instructions and tools. | On | `long_cache` | No |
| **Step plan** | A visible plan with a percentage for multi-step work. | On | `plan_enabled` | Yes, composer |
| **Sub-agents** | Offer clean-context helpers. | Off | `subagents_enabled` | Yes: follow, on or off |
| **Wrap pasted code in a fenced block** | Pasted code becomes a fenced block with its language. | On | `wrap_pasted_code` | No (use Ctrl+Shift+V once) |
| **Project instructions** | Read `CLAUDE.md`, `AGENTS.md` and your own instructions file. | On | `use_instructions` | No |
| **Chat only** | New conversations are chats with no tools. | Off | `chat_only` | Chosen when the conversation starts (Agent / Chat switch) |
| **Mode** | Manual, Accept edits or Autonomous. | Manual | `mode` | Yes, composer or palette |
| **Decisions** | Ask at forks, Only big decisions, Decide itself. | Ask at forks | `decision_level` | Yes |
| **Your instructions about decisions** | Free text, up to 2,000 characters. | Empty | `decision_notes` | Yes: its own text is appended |
| **General context** | Standing notes about you (role, stack, tone, language, rules) added to Imber's instructions in every conversation, agent or chat, inside a `` block. Written as a list, one note per item (`Enter` adds the next, `Alt+↑` / `↓` moves one); up to 4,000 characters in all. Edit it from this page, the panel's ⋯ menu or the command palette (*Imber: General context…*); Ctrl/Cmd+Enter saves. The same modal has a second tab, *This project*: notes about the open repository (conventions, what not to touch), kept in `.arbor/bennu/config.toml` under `[assistant]` and sent in a `` block only with conversations about it, under the same cap. What a whole team should read belongs in `CLAUDE.md`. | Empty | `general_context` | No |
| **Debug session idle time** | Minutes before an unused debug session is ended (5 to 240). | 30 | `debug_idle_minutes` | No |
| **Notifications › When it waits for you** | A system notification, and the Arbor icon asking for attention, when Imber stops on a question, an approval or a card while no Bennu window is in front. | On | `notify_questions` | No |
| **Notifications › When a run ends** | The same when a run finishes or stops on an error while no Bennu window is in front. | Off | `notify_done` | No |
| **Confirm costly changes** | Ask before a model or window change that makes a long conversation re-read its history at full price. | On | `confirm_cost_changes` | No |

## What it may reach

| Setting | What it does | Default | Key | Per conversation |
| --- | --- | --- | --- | --- |
| **Reach** | This file, this project, all open projects, anywhere on disk. | This project | `scope` (`file`, `project`, `workspace`, `disk`) | Yes, composer |
| **Projects at once** | With the reach "all open projects": how many projects one conversation works across (1 to 20). Projects Imber brings in count too. | 6 | `max_projects` | No |
| **Opening other projects** | When Imber needs Bennu on a folder that is not open in Bennu (indexed where it can be: a Java build, or a language server) but that it may already read: *Ask, except in Autonomous* (`autonomous`), *Always ask* (`ask`), *Open where it may read* (`allowed`) or *Never* (`never`). A project open in Bennu joins without this question; one outside the reach is asked about with the access card. See Imber › Tools, Several projects. | Ask, except in Autonomous | `open_projects` | No |
| **Edit files** | Off makes Imber read-only. | On | `allow_edits` | Yes, composer › Permissions |
| **Use the Temp folders** | Scratch space outside the project. | On | `allow_temp` | Yes, composer › Permissions |
| **Folders it may work in, besides the project** | Extra folders allowed in any reach. Add folder… opens the folder picker. | None | `extra_folders` | No |

## Tools it may use

| Setting | What it does | Default | Key | Per conversation |
| --- | --- | --- | --- | --- |
| **One switch per family**: Read and search files, Project intelligence, Edit files, Refactoring, Structural search and replace, Local History, Build run and test, Debugger, Look into a running Bevy game, Screenshots of a page, Live services | A family that is off is not offered at all. | All on | `tool_families_off` (a list of the families switched off) | No |
| **Programs it may run** | Programs on this machine Imber may call by name with `run_tool`. Each has a name, a description Imber reads, the program's absolute path (*Choose…* opens the explorer), fixed arguments (one per line) and variables (`KEY=value` per line), whether Imber may add arguments, which variables it may set, whether it changes things, when a call asks (every call, once per conversation, never), a time limit in seconds and an on/off switch. A tool the run would not offer is marked with the reason. In the Build run and test family. | None | `external_tools` | No |
| **Let it change a running Bevy game** | Mutate, insert, remove, spawn, despawn, set a resource. Needs the Bevy family. | Off | `bevy_write_enabled` | No |
| **Let it change the program it debugs** | Throw, return early, drop a frame, write a value. Needs the Debugger family. | Off | `debug_mutate_enabled` | No |
| **Start a run when the program stops** | When the debugged program stops and Imber is idle, start a run (at most 3 an hour per conversation). | Off | `auto_continue_on_stop` | Yes, composer › Decisions |

## Skills

| Setting | What it does | Default | Key | Per conversation |
| --- | --- | --- | --- | --- |
| **Skills…** | Opens the skills manager and shows "N of M enabled". | None enabled | `skills_on` | No |

## Settings that live only in the composer or the panel

- **Isolated copy** (off, copy, git worktree) and **Build isolation** (shared queue or separate target): chosen per conversation, fixed once it starts.
- **Dry run**: Send shows the request instead of making it.

---

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
| *Imber: Schedules…* | always |
| *Imber: Schedule this prompt…* | the composer has text and a project is open |
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

---

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

---

# Imber: Troubleshooting

The messages Imber and Bennu show most often, what each one means, and what to do.

## "No API key is set. Add one in Settings › Imber."

Open Settings › Imber › Connection, paste your Anthropic key and press Save. See Imber › Getting started.

## "Open a project first — Imber works inside one"

Agent conversations need an open project. Open one, or switch to Chat for a question that has nothing to do with a project.

## A run stopped with a limit message ("Raise the limit in Settings › Imber › Spending limits to go on")

A conversation, day, week or month reached its limit in euros. Raise the limit under Settings › Imber › Spending limits, or set it to 0 to switch it off. See Imber › Cost and context.

## "… is outside what you allowed. Open it for reading, or for reading and writing?"

Imber reached for a file outside the reach. Choose Deny, Allow reading or Allow reading and writing; the answer covers that folder for the session. To avoid the question, widen the reach or add the folder under Settings › Imber › What it may reach. See Imber › Permissions and modes.

## "The user has switched that kind of tool off" / "this is a chat"

The family is off in Settings › Imber › Tools it may use, or the conversation is a Chat. Turn the family on, or start an Agent conversation.

## "Editing is switched off"

Edit files is off (Settings › Imber, or composer › Settings › Permissions). Imber is read-only until you turn it on.

## "… holds secrets and is never written" / "the repository's own .git files are never changed here"

Hard limits. Edit the file yourself.

## The status line says "No connection - retrying" (or "The API is overloaded", "Rate limit reached")

A request failed for a reason that usually passes, and Imber is waiting to send it again. The line shows the attempt and the seconds left; **Retry now** skips the wait and **Stop** ends the run. When the connection comes back the line disappears and the answer continues; nothing you did in between is lost. See Imber › Cost and context › When a request fails.

## A run ended with "You are offline", "The API stopped answering" or "The connection dropped"

Imber waited for the connection for about three minutes and it did not come back. Everything that had finished is saved. Fix the connection (or the VPN/proxy), then press **Retry** under the message: the same request is sent again, with no new message from you.

## "The API key was rejected"

The key is invalid, revoked, or may not use that model. Open Settings › Imber › Connection, save a valid key, then press **Retry**. Nothing is retried automatically: waiting would not help.

## "The account has no credit left or reached a usage limit"

The provider refused because of the account, not Imber's own spending limits. Check the plan and billing of the provider account, then press **Retry**.

## "The conversation no longer fits the model's window"

Compact the conversation (the panel's **…** menu › Compact), or start a new one, then press **Retry**. See Imber › Cost and context.

## "The rate limit was reached" / "The API is overloaded"

Imber already tried five times, waiting at least as long as the provider asked. Wait a minute and press **Retry**, or use a lighter model or effort.

## A sub-agent failed with a connection error

The agent's card says what failed; the conversation that started it is told the same, including whether the agent had already changed files. Ask for the agent again; its earlier changes stay.

## The proposal card says Outdated

A file changed after Imber read it. Nothing was written. Use Re-apply on current file, Ask to redo, or Discard. See Imber › Reviewing changes.

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

Check the context meter's breakdown. Compact the conversation, start a new one, lower the effort, switch off families you do not need (screenshots are costly), or use a cheaper model. See Imber › Cost and context.

## Answers ignore a project rule

Check that Settings › Imber › Project instructions is on and that the file is among the chips above the composer. Files are cut at 16 KB each and 40 KB together.

## A skill is not used

A skill is invisible until enabled in the skills manager. Check Imber › Skills for the folders, the format, and the size limits.

## "Imber could not write a diagram that parses"

After three tries the source is shown as text. Copy it from the notice, or ask Imber to simplify the diagram.

## A debug session ended by itself

An unused session ends after the idle time in Settings › Imber (30 minutes by default), when the program exits, or when the project closes. See Imber › Debugging with Imber.

## Imber says it cannot do something Bennu can

It acts only through its tools: no shell, no push, nothing outside the reach. Check Imber › Tools it can use for the tool, and the family switch.
