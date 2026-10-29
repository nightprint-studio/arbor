---
title: "Imber: Tools it can use"
summary: "Every family of tools with its purpose, how to switch one off, git access and several projects."
keywords: ["tools", "families", "git", "hide authors", "projects at once", "help", "digest", "switch off"]
related: ["Imber: Permissions and modes", "Imber: Running and observing", "Imber: Settings reference"]
source: src/lib/components/bennu/docs/imber/Tools.svelte
---
<!-- Generated from the in-app docs - do not edit. Edit the Svelte pages and run `node scripts/build-help-index.mjs`. -->

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
| **Build, run and test** | `bennu_build`, `bennu_test_run`, `bennu_cargo_run`, `bennu_npm_run_script`, run configurations, launching a program | Anything that executes the project. See [Imber › Permissions and modes](03-permissions.md) for when each asks. |
| **Live services** | `run_start`, `run_status`, `run_logs`, `run_stop`, `wait_ready`, `list_recipes`, `run_recipe`, `http_request`, `trace_request`, `timeline`, `list_scenarios`, `run_scenario`, `save_scenario`, `console_logs`, `tail_file`, `wait_log`, `log_facets`, `log_record` | Start the project's services, read their logs, call them over localhost, follow one request through every log, run scenarios. See [Imber › Running and observing](06-running-and-observing.md). |
| **Debugger** | `debug_set_breakpoint`, `debug_list_breakpoints`, `debug_remove_breakpoint`, `debug_launch`, `debug_attach`, `debug_wait_stop`, `debug_stack`, `debug_variables`, `debug_evaluate`, `debug_step`, `debug_resume`, `debug_detach`, `debug_output`, `debug_run_to`, `debug_break_on_exception`, `debug_select_thread` | Debug a Java program. See [Imber › Debugging with Imber](07-debugging.md). |
| **Look into a running Bevy game** | `brp_connect`, `brp_query`, `brp_get`, `brp_resources`, `brp_get_resource`, `brp_types` | Read entities, components, resources and types over the Bevy Remote Protocol. Only for projects with a `Cargo.toml`. See [Imber › Bevy games and the profiler](08-bevy-and-profiler.md). |
| **Screenshots of a page** | `screenshot_page`, `page_diagnostics` | Look at a front end on localhost. See [Imber › Screenshots and diagrams](09-screenshots-and-diagrams.md). |

## Switches that are not in the family list

- **Let it change a running Bevy game** (`brp_mutate`, `brp_insert`, `brp_remove`, `brp_spawn`, `brp_despawn`, `brp_set_resource`): off by default, needs the Bevy family on.
- **Let it change the program it debugs** (`debug_throw`, `debug_force_return`, `debug_drop_frame`, `debug_set_value`): off by default, needs the Debugger family on.
- **Git** (`corvus_status`, `corvus_diff`, `corvus_log`, `corvus_blame`, `corvus_stage`, `corvus_unstage`, `corvus_commit`): off by default. See below.
- **Sub-agents** (`spawn_agent`, `resume_agent`, `await_agents`): off by default; and the peer tools `file_owner` and `request_change` for talking to other conversations about a file. See [Imber › Sub-agents](11-sub-agents.md).
- **Step plan** (`update_plan`): on by default.

## Tools that are always there

`ask_user` (questions, [Imber › Decisions and questions](12-decisions-and-questions.md)), `bennu_help` (the help about Bennu itself), `open_ui` (opens a panel or window), `show_diagram` (draws a Mermaid diagram), `load_skill` (only when you enabled a skill, [Imber › Skills](10-skills.md)) and `expand_result` (returns the detail of a digest). In a chat only `bennu_help`, `open_ui` and `show_diagram` are offered.

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
