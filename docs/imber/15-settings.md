---
title: "Imber: Settings reference"
summary: "Every Imber setting: what it does, its default, the config key and whether a conversation can override it."
keywords: ["settings", "config", "config.toml", "default", "key", "vault", "assistant-models.toml"]
related: ["Imber: Getting started", "Imber: Cost and context", "Imber: Permissions and modes"]
source: src/lib/components/bennu/docs/imber/Settings.svelte
---
<!-- Generated from the in-app docs - do not edit. Edit the Svelte pages and run `node scripts/build-help-index.mjs`. -->

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
