# Imber schedules in Bennu — design

Status: **implemented** (steps 1–4; 5 and 6 are not built). §8 records the decisions taken: 1 app config dir, 2 extract, 3 Imber always asks, 4 default caps, 5 Hirundo shares the crate, 6 only while Arbor runs, 7 no `autonomous`. Where the code differs from the text below, the notes in §10 win.

Imber (the assistant shared by Bennu and Corvus) can already be scheduled in **Corvus**: "every weekday at
09:00 ask Imber to summarise what changed on `main`". This document is about giving Bennu the same, without
writing the machinery a second time.

---

## 1. What exists today

| Where | What | Notes |
|---|---|---|
| `arbor-scheduler::calendar` | `Recurrence` (once / interval / daily / weekly / cron, local time), `next_after`, `due`, `MissedPolicy` | Pure and tested. Already shared. |
| `corvus-be/src/assistant/schedule/` | `model.rs` (a job as data), `store.rs` (list mirrored to `imber-schedules.json`), `mod.rs` (handlers for the window, `schedule_*` tools for Imber, MCP tools, the 20 s loop) | Complete. Actions: `Awake`, `Prompt`, `Fetch`. |
| `corvus-be/src/assistant/permission.rs` | `unattended_allows(mode, risk, tool)` | The rule for a run with nobody to approve. |
| `hirundo-be/src/schedules.rs` | Its own `Schedule` + tick, on the same `calendar` | A third shape of the same idea. |
| `bennu-be/src/assistant/` | Runs, conversations, proposals, access modes (`manual` / `accept_edits` / `autonomous`), isolated worktree, spending limits | **No schedules**, and no notion of an unattended run: every run assumes a person at the panel. |

So the question is not "can Imber be scheduled" — it is how much of Corvus's implementation is product-neutral
(most of it) and what Bennu has that Corvus does not (code execution, a three-level access model, isolated
worktrees).

## 2. Goals and non-goals

Goals

- A Bennu user can schedule a **self-contained Imber prompt** on a project, once or on a repeat, see when it runs
  next and how the last run ended, and open the conversation it produced.
- Imber itself can create, change and remove schedules from a conversation (with the user's consent), and MCP
  clients can list and create **read-only** ones — same surface as Corvus.
- An unattended run can never do something a person would have been asked about.
- One implementation of "a schedule as data", used by Corvus and Bennu.

Non-goals

- A background daemon. Like Corvus and Hirundo, a schedule fires **only while the product's backend is running**
  (the Bennu window for that project is open). A run missed meanwhile is skipped or run once, per schedule.
- Schedules that start other schedules (a scheduled run cannot touch `schedule_*`).
- Migrating Hirundo's schedules now (§8, question 5).

## 3. Architecture

### 3.1 Extract the neutral part into `imber-schedule`

New library crate `crates/platform/imber/schedule` (next to `core` and `transport`), exposed through `prelude`:

| Piece | From | Notes |
|---|---|---|
| `Job<A>` — id, name, enabled, `Recurrence`, `MissedPolicy`, `root`, `mode`, next/last run, run count, `action: A` | `corvus … schedule/model.rs` | `A` is the product's action enum. |
| Life of a job: `schedule_from`, `tick`, `started`, `finished`, `due_now` | same | Pure, clock as a parameter — the existing tests move with it. |
| `Store<A>` — in-memory list, atomic JSON file (temp + rename), a damaged file is not overwritten until the next save | `… schedule/store.rs` | The file path is a parameter. |
| `when_to_recurrence(&Value, now_ms)` — the friendly `{"kind":"weekly","days":[…],"time":"09:00"}` form | `… schedule/mod.rs` | Imber writes this in tool calls. |
| `ToolSpec` builders for `schedule_list/create/update/delete` | same | The descriptions differ slightly per product (the actions on offer), so builders take the action list. |
| Validation limits (`MAX_PROMPT_CHARS`, name length, minimum interval) | same | |

What stays in each product: the `Action` enum, the permission table, how a run is started, the loop (a thread in
the BE, `TICK` = 20 s), the handlers.

Corvus is migrated first (step 1 below), with its tests, so the extraction is proven on code that already works
before Bennu depends on it.

### 3.2 Bennu: `bennu-be/src/assistant/schedule/`

- `Action` for Bennu: `Awake { conversation_id, prompt }` and `Prompt { prompt }` in v1. Candidate no-model
  actions for later, the way Corvus has `Fetch`: `Test { scope }`, `Check` (project validation) — they cost
  nothing and give the schedule a result without a model.
- `run_job` builds the same `SendArgs` a panel send builds, with `unattended = true`, the job's model, effort,
  scope and mode, and an end hook that records the outcome on the job (the shape of Corvus's `DoneHook`; Bennu
  already maps `Ended` to `done / cancelled / too_many_steps / incomplete / failed`, so the hook has the words).
- The loop starts on the first list call, as in Corvus, and its first look is immediate: that is where missed
  runs are dropped or caught up.

### 3.3 Where the list lives

`<bennu config dir>/imber-schedules.json`, keyed by `root`, **not** inside the repository: a job's next/last run
change on every fire and its prompt is personal, so a file under `.arbor/` would show up as noise in `git status`
and leak into the team. (Open question 1.)

## 4. Autonomy: what an unattended run may do

A scheduled run has nobody to approve anything. Bennu has more power to misuse than Corvus (it runs the
project's build and tests, edits many files, can start processes), so the rule is stricter than "read-only or
auto":

| Capability | Unattended |
|---|---|
| Read the project, search, index queries, history | always |
| Propose an edit | allowed; the proposal waits as a card in the run's conversation. Nothing is written to the live project. |
| Write to the live project | **never** |
| Write to an **isolated copy** (the existing worktree mode) | allowed when the job has `isolated = true`; the result is the usual **Apply to project** review, left pending |
| The project's own build and test commands | off by default; the user can allow them **per job** in the Schedules window (never Imber, never MCP) |
| Arbitrary commands, scripts, `run_start`, debugging | never |
| Delete / rename files | never |
| A question to the user | not asked; the tool answers "scheduled run, nobody to ask — decide conservatively or say it needs a person", and a run that still cannot go on ends `incomplete` with that line as its outcome |
| `schedule_create/update/delete` | never (`schedule_list` is fine) |

Why isolated copies instead of an `autonomous` mode on a schedule: Bennu already has the mechanism that makes
"let it edit freely" safe — a copy that is reviewed and applied or discarded. A schedule that wants to change
code uses that; the live tree is never touched by something nobody watched.

A job Imber creates is **no freer than the conversation that created it** (Corvus's rule), and it is created only
after an approval card that spells out the schedule in words ("Every weekday 09:00 · read-only · asks Imber:
…"): it creates future behaviour, so it is asked about in every mode below autonomous.

## 5. Cost and noise

- **Per-run cap** (`max_cost_eur`, small default) ends a run as `incomplete` when reached.
- Unattended spend counts against the **same limits** the panel meters already show, plus a cap on scheduled
  runs per day, so a mis-set cron cannot burn the month's budget overnight.
- A job never starts while its previous run is still going (skip, recorded as "skipped: still running").
- `MIN_INTERVAL_SECS` from the calendar module stays the floor for interval jobs.
- The run's model is the user's default, with a "cheaper model" choice on the form (the side-model setting
  already exists for compaction).
- Result delivery: the conversation (titled `[Scheduled: name]`), the one-line outcome on the schedule, and **one**
  toast per failing episode (the pattern used by the other auto-reconnect loops: first failure, not every tick).

## 6. UI and keyboard

- The Schedules window and form today live under `corvus/`. They become shared: the window as a cross-product
  modal in `shared/`, the form as a widget in `shared/internal/` built only from `shared/ui` widgets, driven by
  the product's list of actions and its extra fields (Bennu: `isolated`, allowed commands, model). One pattern,
  not two copies.
- Command palette: **Imber: Schedules…** and **Imber: Schedule this prompt…** (from the composer, prefilled).
  Shortcut proposal: none for v1 — rare action, the palette is enough; if wanted, `Ctrl+Shift+<letter>` after
  checking `keybindings.ts`.
- Keyboard flow of the form: name auto-focused; Tab through when / prompt / options; `Ctrl+Enter` saves; `Esc`
  cancels; the window list navigates with ↑↓, Enter edits, Delete asks to confirm in a real modal.
- Docs: `docs/imber/` gets a *Schedules* page (the Corvus one in `imber-corvus.md` is the reference), plus the
  in-app docs pages and a `CHANGELOG.md` entry.

## 7. Steps

| # | Step | Done when |
|---|---|---|
| 1 | Create `imber-schedule` (model, store, `when_to_recurrence`, spec builders) and switch Corvus to it | The Corvus schedule tests pass from the new crate and Corvus behaves exactly as before (same file, same tools). README + `prelude`. |
| 2 | Bennu BE: `assistant/schedule` with `Prompt` / `Awake`, the loop, the window handlers, and the unattended gate in the tool host (§4) | An unattended run answers a read-only prompt; a write attempt is refused with the stated message; a question ends the run `incomplete`. Table-driven tests for the gate. |
| 3 | `schedule_*` tools for Imber (approval card) and the two MCP tools | Imber creates a read-only schedule after approval; MCP-created ones are read-only. |
| 4 | Shared Schedules window + form, palette entries, Bennu entry points | Create, edit, disable, run now and delete are doable from the keyboard. |
| 5 | `isolated` jobs and the per-job allowed commands | A scheduled edit lands in an isolated copy with "Apply to project" pending; build/test only when the job allows them. |
| 6 | No-model actions (`Test`, `Check`) | A schedule runs the tests with no model and records the result. |

Steps 1–4 are the useful core; 5 and 6 can wait for a real need.

## 8. Decisions needed

1. **Where the list lives**: app config dir keyed by root (proposed, as Corvus) or per repository under `.arbor/`.
2. **Extract `imber-schedule`** (proposed) versus copying Corvus's module into Bennu. Copying is faster for v1 and
   becomes a third near-identical implementation; extraction costs step 1 but is what CLAUDE.md asks for.
3. **Creating a schedule from Imber always asks** below autonomous (proposed) versus following the write rule like
   any other write tool.
4. **Default caps**: per-run cost, scheduled runs per day.
5. **Hirundo**: leave its schedules alone (proposed) or move them onto `imber-schedule`'s store later.
6. **"Only while Bennu is open"** is acceptable for v1 (proposed) — a real background run is a different product
   decision.
7. **No `autonomous` on a schedule** (proposed): live-tree writes are never unattended; the isolated copy is the
   way to let a schedule edit.

## 9. Tests

All the new logic is pure and gets unit tests: the job's life with a fake clock (due, missed, run once, skip),
`when_to_recurrence` over every kind and its errors, the unattended gate as a table (tool × risk × mode ×
isolated × allowed commands), the store (atomic write, damaged file kept, concurrent mutate), and the effective
mode of an Imber-created job. The loop's thread, the HTTP transport and the window are glue and are not unit
tested.

## 10. As built

Differences from the proposal above, and what was added:

- **The neutral part is not an `imber-schedule` crate of its own.** The job, its store and `when_to_recurrence` are
  `arbor-scheduler::jobs`, next to `calendar`, because Hirundo's runner schedules (not AI) use them too (decision 5):
  `Job<A>` with a product `JobAction`, `Store<A>` with an optional reader for an older file format. The Imber half —
  the `schedule_*` tool specs and runner (`Flavour`), the approval-card words, the window helpers, the loop and the
  events — is `imber-schedule` (`crates/platform/imber/schedule`). Corvus, Bennu and Hirundo each keep only their
  action and how a run starts.
- **Hirundo** keeps the window's wire shape (`Schedule` / `ScheduleView`) and converts at the edge; its per-project
  file is now the job format and a file in the old format (a bare array of schedules) is read, so nothing is lost.
  `schedules_due` now marks what it returns as started, so asking twice no longer returns the same schedule twice.
- **Missed runs leave a trace.** A dropped missed run records a "Missed…" last result (`skipped: true`) for recurring
  jobs too, not only for one-time ones; the UIs show it as neutral, not as a failure.
- **Bennu's gate** is the table in `assistant/schedule/unattended.rs` (kind → allowed / refused with a message to act on),
  fed by `classify_unattended` in `tools/gate.rs`. On top of it, `confirm` and `ask_access` refuse outright for an
  unattended session, so a path the table missed still cannot reach a card nobody will answer. A question ends the run
  `incomplete` with `Needs a person: …` as its outcome; the cost cap does too.
- **A scheduled run is not offered** the tool families it could not use (`run`, `live`, `debug`, `bevy`, `screenshot`,
  `agents`, `peers`, `profile`): cheaper and clearer for the model.
- **Caps**: `limits.schedule_run_eur` (default 1.0 EUR, 0 = off) and `limits.schedule_runs_per_day` (default 24, 0 = off)
  in the shared `[assistant.limits]`. The run cap is enforced in Bennu only; the day, week and month limits apply to
  scheduled runs like to any other. Starts are counted in `<bennu data dir>/assistant/schedule-runs.log`.
- **Creation from Imber always asks**, in every Bennu access mode (autonomous included) and, since the rule is the
  user's, in Corvus's auto mode too.
- **Toasts**: one when a run starts, one on the first failure of an episode (`arbor://<product>/imber-schedule`).
- Not built: isolated jobs and per-job allowed commands (step 5), no-model `Test` / `Check` actions (step 6).
