---
title: "Imber: Running and observing"
summary: "Managed processes, recipes, scenarios, requests, log reading, the shared build queue and build-error attribution."
keywords: ["live", "process", "recipe", "scenario", "logs", "timeline", "build queue", "build isolation", "who broke the build"]
related: ["Imber: Tools it can use", "Imber: Debugging with Imber", "Imber: Permissions and modes"]
source: src/lib/components/bennu/docs/imber/Running.svelte
---
<!-- Generated from the in-app docs - do not edit. Edit the Svelte pages and run `node scripts/build-help-index.mjs`. -->

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
