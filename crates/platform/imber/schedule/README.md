# imber-schedule

Imber's schedules, written once for every product that has them (Corvus, Bennu): "every weekday at 09:00 ask
Imber to summarise what changed".

The *data* is not here. A job (`Job<A>`), its store (one atomically-written JSON file), the `when` an assistant
writes and the calendar arithmetic are `arbor-scheduler`'s `jobs` and `calendar` modules, which Hirundo's runner
schedules use too. This crate is the Imber layer on top:

| File | What |
|---|---|
| `flavour.rs` | `Flavour` — what differs per product: the action enum, the words in the tool descriptions, how a mode is lowered to fit the conversation. Two implementations is what justifies the trait. |
| `tools.rs` | The `schedule_list/create/update/delete` tool specs and `run_tool`, which turns a call into a saved job. `unattended_may_call`: a scheduled run may list schedules, never change them. |
| `window.rs` | What the Schedules window's handlers do (`save`, `set_enabled`, `record`); the handlers stay in the product and are one line each. |
| `driver.rs` | The loop thread (`TICK` = 20 s) that fires what is due, with an immediate first look. |
| `events.rs` | The two events the frontend hears outside any conversation: a run started, and the first failure of an episode (the topic is the product's). |
| `describe.rs` | A schedule in words for an approval card (`describe_when`, `brief`) and the one-line job view a tool answers with. |

What stays in the product: the `Action` enum, the permission table for an unattended run, how a run starts and
what a run's end writes on the job, the handlers (`#[arbor_rpc::handler]` binds to the product's state), and the
approval rule for creating a schedule.

Depends on `arbor-scheduler` and `imber-core` (only for `ToolSpec`). Must not depend on a product.
