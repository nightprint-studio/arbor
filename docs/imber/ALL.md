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
- Imber: Running and observing: Managed processes, recipes, scenarios, requests, log reading, the shared build queue and build-error attribution.
- Imber: Debugging with Imber: Breakpoints, launching and attaching, stops and snapshots, and the off-by-default tools that change a running program.
- Imber: Bevy games and the profiler: Looking into a running Bevy game over BRP, changing it behind a separate switch, and reading a profiler recording.
- Imber: Screenshots and diagrams: Seeing a front end on localhost and drawing Mermaid diagrams in the thread.
- Imber: Skills: SKILL.md folders that teach Imber a kind of task: format, locations, the manager, limits.
- Imber: Sub-agents: Clean-context helpers for side tasks: when to use them, permissions, cost cap, how to watch and stop them.
- Imber: Decisions and questions: How much Imber decides alone (ask at forks, only big decisions, decide itself) and how its questions work.
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
- Cards for proposals (Imber › Reviewing changes), questions, access prompts, the step plan, sub-agents, live runs and debugger stops.
- Notices in the thread: a compaction, a limit reached, an access grant, a system note about another conversation.

## The step plan

For work with several steps Imber writes a plan first and ticks the steps off. The card lists them (done, in progress, still to do) with a bar and "3 of 7 · 43%". The percentage is steps done over steps planned: it says how much of the list is done, not how long the rest will take, and it goes down if the plan grows. The plan costs a few hundred output tokens per update, a few cents for a task of several steps. Questions and small changes get no plan. **Step plan** in the composer's settings menu (and in Settings › Imber) switches it off; the tool and its instruction are then not sent at all.

## Several conversations at once

Two conversations can run on the same project. A file one of them is changing is reserved to it until its run ends: the other is refused, naming the conversation, and can wait or work elsewhere (reading is never blocked). Services and debug sessions belong to the conversation that started them. Builds queue. See Imber › Running and observing.

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
| **Build or run the tests of the project (`bennu_build`, `bennu_test_run`)** | Asks on every call | Asks once per run | Free |
| **Other things that execute code (cargo run, npm scripts, run configurations, launching a program)** | Asks | Asks | Asks |
| **Other Bennu tools that change something** | Asks | Free | Free |
| **Start a process (`run_start`)** | Asks every time | Asks every time | Asks every time |
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
| **A path outside the reach** | One prompt: Deny, Allow reading, Allow reading and writing, for that folder, for the session | Same | Same, except the build-error case below. "Anywhere on disk" never asks |
| **A file named by an error of Imber's own build or check (inside the project)** | The prompt says the build reported an error there | Same | Opened for reading and writing without a prompt, for that run, shown with a Revoke button |
| **Read a secret file (`.env`, keys)** | Asks every time | Asks every time | Asks every time |
| **Write a secret file, or anything under `.git`** | Refused | Refused | Refused |
| **Ask you a question (`ask_user`)** | Always possible; see Imber › Decisions and questions | Same | Same |

A grant you give (reading, or reading and writing) lasts for the session and applies to that folder. A sub-agent's grants are its own and do not carry back to the conversation; see Imber › Sub-agents.

## Autonomous and the build-error files

Under Autonomous, a file named by an *error* of a build or check Imber ran itself (`bennu_build`, `bennu_check_file`, `bennu_problems`) and located inside the project you are in is opened for reading and writing, for that run only, without a prompt. Limits: at most 25 files a run; never anything under build output or fetched code (`target`, `build`, `out`, `dist`, `node_modules`, generated folders); never a dependency manifest or lockfile (`pom.xml`, `Cargo.toml`, `package.json`); never a secret. The thread shows each one ("Access granted to … because the build reports an error there") with a *Revoke* button, and the access ends with the run. Files of another project open this way only when the reach is *all open projects*; otherwise it asks. In Manual and Accept edits the prompt for such a file says why it is being asked.

## Revoking access

Press *Revoke* on an access card to take back a folder before the session ends. Changing the reach to a narrower one takes effect from the next call. Ending the run ends the Autonomous grants.

## Hard limits no mode changes

- No shell. Imber cannot run an arbitrary command; processes start only through `run_start` (asks), recipes and scenarios (ask), or the project's own build and test commands.
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

---

# Imber: Tools it can use

Imber acts only through a fixed set of tools, grouped in families; each family can be switched off in Settings, and a family that is off is not even offered to the model.

## The families

| Family (Settings label) | Tools | What it is for |
| --- | --- | --- |
| **Read and search files** | `search_text`, `find_files`, `bennu_read_file`, `read_files`, `file_outline` | Read a file (or several in one call), see a file's declarations with their line ranges, search text, find files by name. |
| **Project intelligence** | `bennu_find_symbol`, `bennu_find_usages`, `bennu_references`, `bennu_callers`, `bennu_symbol_at`, `bennu_outline`, `bennu_check_file`, `bennu_problems`, `bennu_catalog`, `bennu_frameworks`, `bennu_module_graph`, `bennu_project_summary`, `bennu_tests`, `bennu_todos` and more | Bennu's own knowledge of the project: symbols, usages, beans, endpoints, framework catalogues, problems, tests, TODOs, the module graph, the profiler summary. |
| **Edit files** | `edit_file`, `edit_files`, `replace_in_files`, `create_file`, `delete_file` | Exact edits (one or a list across files, all or nothing), text or regular-expression replace over a folder with a dry run and a cap on files, create, delete. |
| **Refactoring** | `rename_symbol`, `add_parameter` | Rename a symbol or add a parameter across every file that mentions it, from the project index, as one change. A renamed public type's file is renamed with it. |
| **Structural search and replace** | `structural_replace` | Rewrite code by its shape, safer than a regular expression. |
| **Local History** | `local_history` | List a file's revisions, show a diff, list deleted files, restore one. |
| **Build, run and test** | `bennu_build`, `bennu_test_run`, `bennu_cargo_run`, `bennu_npm_run_script`, run configurations, launching a program | Anything that executes the project. See Imber › Permissions and modes for when each asks. |
| **Live services** | `run_start`, `run_status`, `run_logs`, `run_stop`, `wait_ready`, `list_recipes`, `run_recipe`, `http_request`, `trace_request`, `timeline`, `list_scenarios`, `run_scenario`, `save_scenario`, `console_logs`, `tail_file`, `wait_log`, `log_facets`, `log_record` | Start the project's services, read their logs, call them over localhost, follow one request through every log, run scenarios. See Imber › Running and observing. |
| **Debugger** | `debug_set_breakpoint`, `debug_list_breakpoints`, `debug_remove_breakpoint`, `debug_launch`, `debug_attach`, `debug_wait_stop`, `debug_stack`, `debug_variables`, `debug_evaluate`, `debug_step`, `debug_resume`, `debug_detach`, `debug_output`, `debug_run_to`, `debug_break_on_exception`, `debug_select_thread` | Debug a Java program. See Imber › Debugging with Imber. |
| **Look into a running Bevy game** | `brp_connect`, `brp_query`, `brp_get`, `brp_resources`, `brp_get_resource`, `brp_types` | Read entities, components, resources and types over the Bevy Remote Protocol. Only for projects with a `Cargo.toml`. See Imber › Bevy games and the profiler. |
| **Screenshots of a page** | `screenshot_page`, `page_diagnostics` | Look at a front end on localhost. See Imber › Screenshots and diagrams. |

## Switches that are not in the family list

- **Let it change a running Bevy game** (`brp_mutate`, `brp_insert`, `brp_remove`, `brp_spawn`, `brp_despawn`, `brp_set_resource`): off by default, needs the Bevy family on.
- **Let it change the program it debugs** (`debug_throw`, `debug_force_return`, `debug_drop_frame`, `debug_set_value`): off by default, needs the Debugger family on.
- **Git** (`corvus_status`, `corvus_diff`, `corvus_log`, `corvus_blame`, `corvus_stage`, `corvus_unstage`, `corvus_commit`): off by default. See below.
- **Sub-agents** (`spawn_agent`, `resume_agent`, `await_agents`): off by default; and the peer tools `file_owner` and `request_change` for talking to other conversations about a file. See Imber › Sub-agents.
- **Step plan** (`update_plan`): on by default.

## Tools that are always there

`ask_user` (questions, Imber › Decisions and questions), `bennu_help` (the help about Bennu itself), `open_ui` (opens a panel or window), `show_diagram` (draws a Mermaid diagram), `load_skill` (only when you enabled a skill, Imber › Skills) and `expand_result` (returns the detail of a digest). In a chat only `bennu_help`, `open_ui` and `show_diagram` are offered.

## Switching a family off

Settings › Imber › Tools it may use has one switch per family. A family that is off is not offered at all: Imber cannot call it, its definitions cost nothing, and a call that slips through is refused with "the user has switched that kind of tool off". "Do not touch my files" is *Edit files* off; "do not run anything" is *Build, run and test* and *Live services* off. The switches are the defaults for new conversations and for the next message of the open one.

## Help about Bennu

Ask how to do something in Bennu (a feature, a shortcut, where a panel is) and Imber looks it up in Bennu's own documentation, this manual, and in a map of the window's buttons and shortcuts, instead of answering from memory. If the documentation has nothing, it says so. It works in a chat too, and so does opening a panel: ask to see the dependencies, the problems or the tests and Imber opens them; in a chat that needs no access to your project.

## Git

Off by default; Settings › Imber › Git turns it on. Imber can then read the repository (status, diff, log, blame) and stage, unstage and commit through Corvus's own git tools. A commit always asks first, in every mode, and it cannot push. Git is not available in a chat.

Reading means commit messages and author names go to the provider. **Hide authors** (on by default) shows the model each person as a made-up id such as `Author-7f3a9c`: the same person is always the same id, so "who wrote most of this" still works. The ids are turned back into names in what you read and in anything Imber sends to git, so a commit message crediting an id credits the real person. The mapping stays on this machine. Git also enables the "Git worktree" kind of isolated copy.

## Several projects

With the reach set to *All open projects* Imber works across the projects open in the workspace, the one you are in first, up to *Projects at once* (6 by default; each is a line in its instructions, so a higher number costs a little on every request). It reads, searches and changes in all of them: a tool takes a project by name (or `name:path`), the text search and the file finder take `project: "all"`, and a change goes to the project that owns the file, which keeps its own Local History. A proposal names a file in another project as `name:path`.

## Big answers are digests

A build with sixty errors, a test run with forty failures, a long search or a long list of usages is answered with a digest (counts, distinct problems grouped, the files most affected and what was left out); `expand_result` returns the detail for one file, test or message. A file Imber already read comes back as one line when unchanged, and as the lines that differ when it changed.

---

# Imber: Running and observing

With the Live services family on, Imber can start the project's services, wait for them, read their logs economically, call them over localhost and run scenarios; builds from every conversation share one queue.

The feature itself (processes, recipes, scenarios, the timeline, correlation ids) is described on the **Live services** page. This page is about how Imber uses it.

## Managed processes

- `run_start` starts a named process with a command, a working folder, optional ports and a readiness check (a port, a log line or a health URL). It asks every time, showing the command.
- `run_status`, `run_logs`, `wait_ready` and `run_stop` inspect, wait for and stop what the conversation started. Reading never asks; stopping what the run started does not ask.
- The processes are the same ones the Live panel lists. Every process records its owner; a conversation stops only its own processes. A name or port that another conversation holds is refused instead of duplicated or killed, and a stack another conversation already runs is attached read-only.
- A yes given to a recipes file in one conversation does not carry to another.

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

Breakpoints Imber sets are saved in the project's breakpoints, drawn with a sparkle in the gutter and badged *Imber* in the Breakpoints list (`Ctrl+Shift+F8`). You can disable or delete them like any other. It never changes or removes one you set, and holds at most 20. A logpoint can say "only when `total > 100`" through a condition and a pass count, and can ask for values to be captured at the stop.

## Reading values cannot change anything

Values are read as *paths* (`order.customer.name`, `items[2].price`) through fields and array slots. Method calls, constructors, assignments and `++` are refused before they reach the program, with or without approval. Variables named like secrets (password, token, secret, apiKey, credential…) are masked. Results are capped: frames, variables and long values are cut and say so.

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

Reading never asks. The connection itself and the protocol are explained on the **Bevy Remote** page.

## Changing a running game

*Let it change a running Bevy game* in Settings is a separate switch, off by default, and it needs the Bevy family on. It adds `brp_mutate`, `brp_insert`, `brp_remove`, `brp_spawn`, `brp_despawn` and `brp_set_resource`. These act on live state with no review and no undo. Imber asks once per conversation ("Let Imber change the running game?"), the yes covers every change in that conversation, and every change is listed in the run's summary. Autonomous mode does not skip the question.

## Reading a profiler recording

The project-intelligence family includes three read-only profiler tools. `bennu_profile_recordings` lists the recordings the Profiler tool window made. `bennu_profile_diff` compares two of them method by method (the share of the run each took before and after, biggest changes first), to check that a change made the code faster. `bennu_profile_summary` is a compact summary of one recording (the newest unless Imber names another): the hottest methods with their source lines, the biggest allocation sites and allocated types, lock contention and garbage-collection pauses. Imber uses it to find the method worth optimising and to propose a change. These three only read: with no recording they tell you to make one first. Recording by hand is described on the **Profiler** page; Imber recording for you is below.

## Recording a running JVM

The **Profile** family (`profile_start`, `profile_stop`) lets Imber record a JVM that is already running on this machine with Flight Recorder, through `jcmd`. **Imber asks at every start**, naming the JVM, because a recording samples a running program (nothing in it is changed, it runs a little slower). Imber names the JVM by process id or by part of its main class; with neither, it gets the list to choose from. One recording per conversation, started in one message and stopped in a later one. `profile_stop` ends the recording, waits until it has been read and saved, and returns its summary; the recording is then in the Profiler window like any other. A recording keeps sampling until it is stopped. Sub-agents never have these tools, and the family can be switched off like any other.

---

# Imber: Screenshots and diagrams

Imber can look at a front end running on localhost, and it draws Mermaid diagrams in the thread when a picture explains better than text.

## Screenshots of a page

The *Screenshots of a page* family lets Imber see a page served from `localhost`, `127.0.0.1` or `[::1]`, and nothing else. `screenshot_page` takes a URL, or a port of a process it started plus a path, and can wait for a selector or a number of milliseconds. `page_diagnostics` reports the console, failed requests and the page text.

- **With Playwright in the project**: the picture plus console errors, failed requests and the page text; it can capture one element or the full page.
- **Without Playwright**: an installed Chrome, Edge, Brave or Chromium gives the picture only.
- A screenshot appears in the thread and enlarges on click. The model keeps only the latest two, and they count in the context bar. A picture over about 3.5 MB is refused.
- No file is read or written and nothing asks; the restriction to this machine is in the tool.

Pictures are expensive in tokens, which is why the family can be switched off on its own.

## Diagrams

When a picture explains better than text (an architecture, a flow, a call chain, the entities of a model) Imber draws a **Mermaid** diagram (flowchart, sequence, class, state, ER, Gantt…) with `show_diagram`, in a chat as well as in an agent conversation.

- Each diagram is checked before it is shown. If the source does not parse, Imber is told the error with its line and sends a corrected one, up to three tries; after that the source is shown as text with a notice. The failed attempts are dropped from what is sent to the model afterwards, so they cost nothing later.
- A diagram follows the theme, is capped to a readable height and scrolls inside its box. Its buttons copy the Mermaid source and open it larger in a window.
- A `mermaid` code block in any rendered answer, or in an answer opened as a document, is drawn the same way.
- The source is capped at 20,000 characters.

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
- `~/.claude/skills`.

A project skill hides a user skill of the same name, and `.bennu` wins over `.claude`.

## The skills manager

Open it from **Skills…** in the panel's **…** menu, the composer's settings menu, Settings › Imber › Skills, or the palette entry *Imber: Manage skills…*. It lists each skill with where it came from, its size and a preview of its text; switches them one by one or all at once; and lists folders it could not use with the reason. The Settings page and the composer show "N of M enabled".

Enabling is an allow-list stored in the config (`skills_on`, ids of the form `user/name` or `project/name`): a skill found on disk but not ticked is invisible to Imber.

## How a skill is used

- Only a one-line index of the enabled skills (name and description) travels with each request.
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

- **Read-only by default.** An agent reads and searches on the cheaper side model (Settings › Imber › Cheaper model for side jobs), and up to three run at once. Imber can go on while they work and collects their summaries later (`spawn_agent`, `resume_agent`, `await_agents`).
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

# Imber: Cost and context

Every request carries the whole conversation, so the cost depends less on how much Imber writes than on how much it re-reads; Bennu keeps that small, shows where tokens go, and stops a run at the limits you set.

## The context meter

Above the composer a bar shows how full the model's window is, split into *tool definitions*, *Imber rules*, *project instructions*, the *skills index*, *files and search results*, and the *conversation*. It stays calm until 75%, turns amber there and red past 90%. Open the line for the breakdown and for how much of the last request came from the provider's cache. Every figure is an estimate (about four characters to a token); the exact count comes back with each answer.

*Show what would be sent…* (the panel's **…** menu) lists the same parts for the next request, in the order the provider reads them, with the cost when the cache is cold and when it is warm.

## Models, prices and the context window

The model list and prices are a file, `models.toml`, built in. **Settings › Imber › Models and prices › Create a price file** creates your own copy (`assistant-models.toml` in Bennu's config folder): an entry with the same id replaces the built-in one and any other id is added, so a price change or a new model needs an edit and a *Reload*, not a new build. An entry gives `[model.price]` (`input`, `output` and optionally `cache_read`, `cache_write_5m`, `cache_write_1h`), the `context_max` window, the `efforts` it accepts and, for a model that charges more for long prompts, `[model.long_prompt]` with `over_tokens` and its own rates. Cost is worked out request by request, with cache reads and writes at their own rates.

The **context window** is the largest window Imber may use in a conversation, 50K up to 1M; a size the model cannot hold is listed but cannot be picked, and changing to a smaller model shrinks the window to fit.

## How the cost is kept down

- **The provider's cache.** Tool definitions, instructions and the conversation so far are cached, so a request pays full price only for what the previous one did not already send. Anything that changes the start of a request starts it over, which is why the open file travels with your message and not with the instructions.
- **Long cache.** By default the tools and instructions are kept in the one-hour cache (Settings › Imber › Keep instructions cached for an hour) instead of the five-minute one. It costs a little more the first time and pays off when you pause for more than five minutes between messages or open another conversation in the same project.
- **Reading the same thing twice.** A file already read comes back as one line when unchanged, and as the lines that differ when changed. `file_outline` and `read_files` help open big files by the part that matters.
- **Big results are digests.** Counts, grouped problems, the files most affected, and a statement of what was left out. `expand_result` gives the detail.
- **Old results are stubbed in a batch.** When many old file reads and search results have piled up they are replaced all at once by one-line stubs saying how to fetch them again, and the thread says so. In a batch, never one per message, because changing a message makes the cache start over from it.

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
- The composer flags a limit from 80% of it (**Warn from**, adjustable) and shows what the conversation has cost.
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
- The **price list** is `assistant-models.toml` and your **own instructions** are `assistant-instructions.md`, both in the same Bennu config folder. Your own skills go in its `skills` folder.
- Conversations, the spending ledger (`usage.jsonl`) and the git author mapping live in the data folder `arbor/data/bennu/assistant`. Isolated copies live in `assistant-worktrees` beside it.

## Connection

| Setting | What it does | Default | Key | Per conversation |
| --- | --- | --- | --- | --- |
| **API key** | Your Anthropic key. Save, or Remove. | Not set | (vault) | No |

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

Each limit field also shows how much has been spent against it.

## New conversations

| Setting | What it does | Default | Key | Per conversation |
| --- | --- | --- | --- | --- |
| **Model** | What a new conversation starts with. | Opus 5.5 | `model` | Yes, composer |
| **Largest context window** | The most Imber may put in one conversation. | 200K | `max_context` | Yes, composer |
| **Effort** | How hard it thinks (Low to Max); disabled for a model with no effort setting. | High | `effort` | Yes, composer |
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
| **Debug session idle time** | Minutes before an unused debug session is ended (5 to 240). | 30 | `debug_idle_minutes` | No |
| **Confirm costly changes** | Ask before a model or window change that makes a long conversation re-read its history at full price. | On | `confirm_cost_changes` | No |

## What it may reach

| Setting | What it does | Default | Key | Per conversation |
| --- | --- | --- | --- | --- |
| **Reach** | This file, this project, all open projects, anywhere on disk. | This project | `scope` (`file`, `project`, `workspace`, `disk`) | Yes, composer |
| **Projects at once** | With the reach "all open projects": how many projects one conversation works across (1 to 20). | 6 | `max_projects` | No |
| **Edit files** | Off makes Imber read-only. | On | `allow_edits` | Yes, composer › Permissions |
| **Use the Temp folders** | Scratch space outside the project. | On | `allow_temp` | Yes, composer › Permissions |
| **Folders it may work in, besides the project** | Extra folders allowed in any reach. Add folder… opens the folder picker. | None | `extra_folders` | No |

## Tools it may use

| Setting | What it does | Default | Key | Per conversation |
| --- | --- | --- | --- | --- |
| **One switch per family**: Read and search files, Project intelligence, Edit files, Refactoring, Structural search and replace, Local History, Build run and test, Debugger, Look into a running Bevy game, Screenshots of a page, Live services | A family that is off is not offered at all. | All on | `tool_families_off` (a list of the families switched off) | No |
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
