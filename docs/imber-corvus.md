# Imber in Corvus

Imber is the assistant of the whole suite. Bennu had it first (code); Corvus now has it for git: commit, push,
fetch, pull, branch, merge, merge requests, and history questions. One setup serves both products.

## What is shared, and how

| Piece | Where it lives | Corvus uses it by |
|---|---|---|
| Agent loop, provider (Anthropic), retry, access policy, price list, message view, `[assistant]` settings type | `crates/platform/imber/core` (`imber-core`, ex `bennu-assistant`) | direct dependency |
| HTTP transport (streamed POST, silence-bounded timeouts, cancel drops the connection) | `crates/platform/imber/transport` | direct dependency |
| API key | shell credential vault (`credential_store::get_assistant_key`) | `host_call("__assistant_key")` per request — never stored or cached (`docs/credential-architecture.md`) |
| Model, effort, spending limits, side-job model | `[assistant]` of Bennu's `config.toml` | read as the file is (`settings.rs`); change them in Bennu's settings |
| Model price list | `assistant-models.toml` beside Bennu's config | same file |
| Mode (manual / auto) | `corvus/imber.toml` | Corvus's own — Bennu's modes are about editing files |

`bennu-be` keeps compiling unchanged: its `Cargo.toml` aliases the package (`bennu-assistant = { package = "imber-core" }`),
`view.rs` / `transport.rs` there are one-line re-exports, and `bennu-core` re-exports the settings types.

## Architecture (Corvus side) — `crates/products/corvus/be/src/assistant/`

```
imber_send ──► thread ──► imber_core::run(Anthropic, HttpTransport, ImberHost)
                                   │  for each tool call:
                                   ├─ host.gate(call)  ── permission::classify / needs_approval
                                   │        └─ write in manual, or any destructive call → approval card, run parks
                                   └─ host.run(call)   ── tools::run → the same handlers as the UI buttons
events: arbor://corvus/imber  { run_id, conversation_id, kind: message | status | usage | retry | retry_end | done }
```

- **No git is reimplemented.** `tools.rs` calls `corvus_status/diff/blame/commit/stage/unstage/push` (the MCP facade),
  `fetch_remote`, `pull_branch`, `push_branch`, `create_branch`, `checkout_branch_safe`, `merge_branch`, `create_mr`,
  `list_mrs`, `delete_branch` — so hooks, recovery snapshots and credentials behave exactly as for a click.
- **Permission** is a table (`permission.rs`): read never asks; write asks in manual and runs in auto; destructive
  (force push, branch delete, anything unclassified) asks in both. The card shows the exact git command and effect.
- **Commit message** (`imber_commit_message`, `commit_message.rs`, `diff_text.rs`): the staged diff within a budget, the
  repository's measured commit style (Conventional / ticket prefix / plain), `commit.template`, the branch's ticket, and
  whatever the user already typed as a hint. It uses the side-job model (cheap by default), fills the box, never commits,
  and is cancellable (`imber_cancel(request_id)`).
- **Frontend**: `ImberPanel` (right panel, `Ctrl+Shift+Y`), `ImberMessage`, `ImberApproval`; store `stores/corvus/imber.svelte.ts`;
  palette verbs "Imber: …"; sparkle + `Ctrl+G` in `StageCommitForm` (opt-in via `onGenerate`, so Bennu's form is unchanged).
  The message/tool-row/author components are Bennu's assistant components, reused as they are.

## Not done (deliberate, to keep this "not too complex")

3. **Day / week / month limits and the shared ledger.** Only the per-conversation limit is enforced; the ledger
   (`bennu-be/assistant/ledger.rs`) is file-based and Bennu-specific. Moving it to `imber-core` + a path-injecting host
   would make the limits one budget across products.
4. **Conversations are in memory** (the transcript lives in the backend for the session; the thread in the window).
5. **Moving the presentational components to `shared/`.** `ImberMessage` imports `AssistantText`, `AssistantToolRow`,
   `AssistantUserText`, `AssistantWho` from `components/bennu/assistant/` — a cross-product import that should become
   `shared/internal/imber/` once Bennu's parallel work settles. Same for `bennu-be`'s own `runs.rs` (cancel + prompt
   registry), which Corvus re-implements in ~60 lines (`assistant/runs.rs`): extract to `imber-core` and use in both.
6. **Settings UI.** Corvus shows the shared choices but does not edit model/effort/limits (writing Bennu's config file
   from another process would race with Bennu's in-memory copy). The key can be stored from the panel (shared vault).
7. **Multi-repo conversations**, resolving conflicts, rebase/reset (never offered), tags, stash.

## Where to look when something misbehaves

- "No API key is set": the vault has none — store one from the panel or Bennu › Settings › Imber.
- A tool answers with a git error string: it is the handler's own message, byte-identical to the UI's.
- Dev failure simulation: `ARBOR_IMBER_FAKE_API_FAILURE=offline:3|429:2|500:5|401:1` (see `imber-transport`).

## Schedules

Imber can run at a time (once, every N minutes, daily, on weekdays, cron) and wake a conversation with a saved prompt
("awake"), start a new one, or fetch. Bennu has the same schedules (actions `awake` / `prompt`, always read-only; see
`docs/imber-schedules-bennu.md`); the code below the product line is shared.

| Piece | Where |
|---|---|
| Recurrence, `next_after`, parsing of times/weekdays, the missed-run rule (`due`, `MissedPolicy`) — pure, tested | `arbor-scheduler::calendar` |
| A job as data and its life (`Job<A>`: `schedule_from`, `tick`, `started`, `finished`), the store (one atomically-written JSON file), `when_to_recurrence` — pure, tested | `arbor-scheduler::jobs` (Hirundo's runner schedules use it too) |
| The `schedule_*` tool specs and runner, the approval-card words, the window helpers, the 20 s loop, the start/failure events — written once against a `Flavour` | `imber-schedule` |
| The `Action` enum (`awake` / `prompt` / `fetch`), the wording of the tools | `corvus-be/src/assistant/schedule/action.rs` |
| Handlers for the window (`imber_schedule_*`), MCP tools (`corvus_imber_schedules`, `corvus_imber_schedule_create`), what a run does when it fires; the list lives in `<corvus config dir>/imber-schedules.json` | `schedule/mod.rs` |
| Window, form, store | `shared/ImberSchedulesModal`, `shared/internal/ImberScheduleForm`, the shared schedules store (one instance per product) |

**Why not register jobs in the engine's in-memory `Scheduler`?** A job must be listed, edited, persisted and answer
"next run / last result"; the engine's entries are opaque closures without those, and its handle lives inside
`arbor-be`. The calendar module is the shared half (one cron parser, one place for next-run rules); the loop is a
20-second look at the list. Consequently these schedules do not appear in the read-only *Active Schedules* inspector
(which lists the engine's registrations: plugins, marketplace) — they have their own window.

**Autonomy.** A scheduled run is *unattended*: `permission::unattended_allows` — read always; write only if the job's
`mode` is `auto`; destructive never; `schedule_*` writes never (no schedule creates schedules). A job created by Imber
gets `min(requested, conversation mode)`; one created over MCP is always read-only. Runs reuse `begin_run` (the same
path as the panel), including the one-run-per-conversation guard and the per-conversation spending limit.
**Creating a schedule from a conversation always shows an approval card**, in manual and in auto mode: it makes
behaviour that happens later with nobody there.

**Missed runs.** The loop exists only while Corvus runs (there is no daemon). The first look is at start-up. Past the
2-minute grace a run is dropped (`skip`, default) or run once (`run_once`); a dropped run leaves a "Missed…" line as
the schedule's last result, and a missed one-time job turns itself off. Start-up of the loop is triggered by the first
`imber_schedule_list`, which the shell window does at mount.

**What the user is told.** A discreet toast when a run starts (`arbor://corvus/imber-schedule`, `kind: started`, with
the schedule's name) and one when a run fails after a good one — one per failure episode, not one per tick. The
outcome of the last run also shows on the schedule.

Not done: schedules for other repositories than the one of the creating conversation from the tool (the window can
edit any), a run history beyond the last result.

## Plans and undo

**Plans** (`assistant/plan_tools.rs`). Imber builds and drives Corvus's existing plans through the plan handlers
themselves (`plan::plan_add_step`, `plan_run`, `plan_stop`, `plan_retry_step`, `plan_skip_step`, `plan_release_gate`,
plus `create_draft`, `set_gate`, `remove_step`): tools `plan_list`, `plan_show`, `plan_create`, `plan_add_step`,
`plan_remove_step`, `plan_set_gate`, `plan_run`, `plan_stop`, `plan_retry_step`, `plan_skip_step`, `plan_release_gate`.
Permissions are the table's: reading is free; building / editing a draft and running it are *writes* — in **manual** the
run is a proposal card with the plan's steps and the Plans window opens on that plan for review, in **auto** it runs.
A scheduled run may run a plan but not one that merges requests. The chat shows a **plan card** (phases, gates, each
step's state and outcome) that follows the live plan (`arbor://plan-updated`), with Retry / Skip / Resume / Stop.

**Undo** (`assistant/undo.rs`). Every change Imber makes is logged (`imber-actions.json`) with how to take it back,
worked out from the repository *before* the action; `undo_last` / the panel button / *Imber: Undo Last Action* run the
newest reversible entry after checking the repository is still as the action left it (HEAD in place, a clean tree for a
hard reset), newest first. Reversible: commit (soft reset), branch create / switch / delete, stage / unstage, merge and
pull that moved HEAD (hard reset, which takes the usual recovery snapshot). **Not** reversible, and listed as such with the
reason: push, merge request opened, plan run. Undo always asks, even in auto.

Still not done: day / week / month spending limits (above); undoing a stash or a rebase (Imber offers neither); a
plan that spans repositories is undone step by step by hand.
