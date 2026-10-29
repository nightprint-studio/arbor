---
title: "Imber: Permissions and modes"
summary: "Manual, Accept edits and Autonomous modes, the four reaches, and one table of what asks when."
keywords: ["mode", "manual", "accept edits", "autonomous", "reach", "scope", "ask", "permission", "delete", "secret", "revoke", "temp", "outside"]
related: ["Imber: Reviewing changes", "Imber: Tools it can use", "Imber: Privacy and safety"]
source: src/lib/components/bennu/docs/imber/Permissions.svelte
---
<!-- Generated from the in-app docs - do not edit. Edit the Svelte pages and run `node scripts/build-help-index.mjs`. -->

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
| **Ask you a question (`ask_user`)** | Always possible; see [Imber › Decisions and questions](12-decisions-and-questions.md) | Same | Same |

A grant you give (reading, or reading and writing) lasts for the session and applies to that folder. A sub-agent's grants are its own and do not carry back to the conversation; see [Imber › Sub-agents](11-sub-agents.md).

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
- Spending limits apply in every mode ([Imber › Cost and context](13-cost-and-context.md)).
