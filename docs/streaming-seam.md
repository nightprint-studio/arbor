# Streaming seam

How long-running, incremental commands deliver their output to the frontend under
Model D — and why a real `tauri::ipc::Channel` is the wrong primitive for that seam.

## The shape, in one sentence

A streaming command **returns an id synchronously**, then pushes a sequence of
**one-way, id-correlated events** at its own pace; cancellation is a **separate
call keyed by that id**. There is no per-call duplex channel.

## Why not `tauri::ipc::Channel`

A `Channel<T>` is a duplex artifact the shell injects into a command's argument
list and the command writes back through. It can't survive the Model-D seam, for
two independent reasons:

1. **Handlers don't see the shell.** A `#[corvus::handler]` / `#[platform::handler]`
   takes `&AppState` and nothing else — no `AppHandle`, no `Webview`, no
   `tauri::ipc::Channel`. The whole point of the broker seam is that a handler is
   reachable identically in-process *and* across a real process boundary
   (`crates/foundation/ipc/src/transport.rs`: `serve_stdio` + `ChildClient`),
   where the only egress is framed JSON. A `Channel` has no representation in that
   frame protocol. Handing one to a handler is impossible by construction.

2. **The transport is one-way for everything that isn't a reply.** Events ride a
   dedicated backend→shell channel as `Event::Notify { topic, payload }` frames
   (`event.rs`, `transport.rs`), modelled on LSP notifications — never on the
   request/response path. The shell coalesces / throttles / re-emits to the FE.
   A duplex `Channel` cuts against that design: it's a second, unmodelled return
   path that the broker can't demux, throttle, or route per-window.

The correct seam, therefore, is the one **five of the six** deferred streaming
commands already use:

> **synchronous id  +  correlated one-way events  +  cancel-by-id**

`get_file_blame_streaming` is the lone holdout still on a real `Channel`
(`commands/diff_commands.rs`, `git/blame_incremental.rs`; FE `src/lib/ipc/diff.ts`
constructs `new Channel<BlameProgress>()`). It must be converted off it. Once it is,
`import { Channel }` disappears from the frontend entirely.

## The `EventSink` foundation

A handler holds an `Arc<dyn EventSink>` (`AppState::event_sink()`), not an
`AppHandle`. In-process the sink is backed by `AppHandle::emit`; a split-out
backend backs it with `FrameEventSink` writing `Event::Notify` frames
(`crates/foundation/ipc/src/transport.rs`). **The call site never changes — only
the backing.** The sink is `Send + Sync` and clones cheaply into a background
thread that outlives the call and emits from inside. `get_workdir_diff_stream`
(`ipc/corvus/diff.rs`) is the reference: it resolves the sink, mints an id, emits
a `started` event synchronously, spawns a thread that emits one event per file,
and returns the id — all without touching an `AppHandle`.

What's missing today is **standardization**. Each command hand-rolls its own topic
strings, its own envelope keys, and its own started/chunk/done/error lifecycle.
`get_workdir_diff_stream` emits `diff-stream-started/-file/-done/-error`;
`start_shell_detection` emits `job-*` plus `shell-detection-done`;
`mr_start_conflict_resolution` emits `mr-conflict-progress/-done`; the cloud
commands emit `cloud-list-chunk` / `cloud-*`. Four ad-hoc dialects of the same
idea, four `listen()` quartets on the FE.

## Proposed: `arbor-ipc::prelude::Stream`

A thin sugar over `EventSink` that standardizes the envelope and the lifecycle.
It does **not** introduce a new transport — every emit is still an ordinary
`EventSink::emit`, so it works in-process and over the frame protocol unchanged.

### Envelope and topics

For a base name `<base>`, a `Stream` emits on four derived topics:

| Topic | When | Payload (beyond envelope) |
|---|---|---|
| `<base>-started` | once, synchronously, before returning the id | `total?`, plus any caller metadata (e.g. the file list) |
| `<base>-chunk` | once per item produced | the item, plus `index?` / `total?` |
| `<base>-done` | once, on success | optional summary |
| `<base>-error` | once, on failure | `error: String` |

Every payload carries a common envelope:

```jsonc
{ "stream_id": "<id>", "seq": 0 /* monotonic per stream, started=0 */ }
```

`seq` lets the FE detect drops / reordering after a reconnect; `stream_id`
correlates the quartet. `started`, each `chunk`, `done`/`error` all carry it.

### Rust handler shape

```rust
#[corvus::handler]
fn get_workdir_diff_stream(state: &AppState, /* … */) -> Result<String, AppError> {
    let sink = state.event_sink().ok_or_else(|| AppError::Other("event sink unavailable".into()))?;

    // Mint the id. Where a job exists, stream_id == job_id (one identity, not two).
    let id = /* job_id from the registry, or a fresh uuid */;

    let stream = Stream::new(sink, "arbor://diff-stream", id.clone());

    // Fast, synchronous phase under the lock → started carries the metadata.
    stream.started(serde_json::json!({ "total": meta.len(), "files": &meta }));

    // Slow phase off-thread; the Stream clones into the closure.
    std::thread::Builder::new().name(format!("arbor-diff-stream-{id}")).spawn(move || {
        match run(|item| stream.chunk(item)) {
            Ok(())  => stream.done(serde_json::json!({})),
            Err(e)  => stream.error(&e.to_string()),
        }
    })?;

    Ok(id)
}
```

`Stream` owns the `seq` counter and the topic-suffix construction; the handler
never spells `-started` / `-chunk` / `-done` / `-error` itself. `chunk` is the only
method that takes producer payload per item; `done`/`error` are terminal and a
debug-assert can guard double-termination.

> **`stream_id == job_id` invariant.** When a command also registers a `JobInfo`
> (diff-stream, shell-detection, conflict-resolution, cloud transfers), the stream
> id **is** the job id. One id addresses the Jobs overlay entry, the stream
> quartet, and the cancel call. Never mint two.

### FE: a single `subscribeStream` / `startStream`

One helper in `src/lib/ipc/stream.ts` replaces both the ad-hoc 4×`listen()`
quartet in `stores/diff.svelte.ts` and the blame `Channel`:

```ts
// Subscribe first, then invoke — so a fast `started` can't race the listeners.
export async function startStream<Chunk>(
  base: string,                       // e.g. 'arbor://diff-stream'
  invokeArgs: { cmd: string; args: Record<string, unknown> },
  handlers: {
    onStarted?: (p: StartedPayload) => void;
    onChunk:    (p: Chunk) => void;
    onDone?:    (p: DonePayload) => void;
    onError?:   (e: string) => void;
  },
): Promise<{ streamId: string; cancel: () => Promise<void>; dispose: () => void }>;
```

`startStream` wires the four `listen()` calls, filters every event by `stream_id`,
invokes the command, captures the returned id, and hands back `cancel()`
(→ `cancel_stream`) plus `dispose()` (unlisten-all). `subscribeStream` is the
lower-level "attach to an id that already exists" variant for the rare case where
the id is known up front. The diff store collapses its four named listeners and
`begin/apply/end/fail` plumbing onto one `startStream` call; blame drops
`new Channel<BlameProgress>()` and treats progress ticks as `chunk`s and the final
line list as the `done` payload — deleting `import { Channel }`.

## Cancellation: `StreamRegistry` + `cancel_stream`

Today cancellation is cloud-only: `AppState::cloud_cancellations`
(`Map<stream_id, Arc<AtomicBool>>`) with bespoke `cloud_cancel` / `cloud_is_cancelled`
handlers flipping the flag (`commands/cloud_commands.rs`). Generalize it:

- **`StreamRegistry`** behind `state.streams()` — a generic
  `Map<id, Arc<AtomicBool>>` that supersedes `cloud_cancellations`. The producer is
  handed a `CancelToken` (a cloneable wrapper over the `Arc<AtomicBool>`) and polls
  it at item boundaries; the registry entry is removed when the stream terminates.
- **One `cancel_stream(stream_id)` handler** replaces `cloud_cancel` — it looks up
  the flag and stores `true`. `cloud_is_cancelled` likewise generalizes (or simply
  goes away on the FE, which only needs to *set* the flag).

A stream that produces no cancellable work (pure egress — see the table) registers
no token; `cancel_stream` on an unknown id is a no-op, exactly as `cloud_cancel` is
today.

## Per-command inventory

| Command | Streams | Egress today | Cancellation | Notes |
|---|---|---|---|---|
| `get_workdir_diff_stream` (`ipc/corvus/diff.rs`) | one parsed `DiffFile` per workdir/index delta | `EventSink` (`diff-stream-*`) | none (parsing is cheap, in-process) | reference impl; already correct shape, refactor onto `Stream` |
| `mr_start_conflict_resolution` (`commands/mr_commands.rs`) | progress phases + per-line output | `AppHandle::emit` (`mr-conflict-*` + `job-*`) | none (pure egress) | pilot: migrate to handler + `Stream`; closes the `mr` domain |
| `start_shell_detection` (`ipc/platform/terminal.rs`) | per-shell detection lines + final list | `EventSink` (`job-*` + `shell-detection-done`) | none | already off `AppHandle`; fold the bespoke topics onto `Stream` |
| `get_file_blame_streaming` (`commands/diff_commands.rs`) | `BlameProgress` ticks, then `Vec<BlameLine>` | **`tauri::ipc::Channel`** | none | the only real-`Channel` user — convert to id + events; ticks → `chunk`, lines → `done` |
| `cloud_list_stream` / `cloud_search_stream` (`commands/cloud_commands.rs`) | object pages / glob matches | `EventSink` (`cloud-list-chunk`) | `cloud_cancellations` → `cloud_cancel` | first consumers of `StreamRegistry` + `cancel_stream` |
| `cloud_download_many` (+ `cloud_report_progress` / `cloud_report_done`) | per-file transfer progress | `AppHandle::emit` (`plugin-operation-*` + `job-*`) | `cloud_cancellations` | jobified; reuse the same id/cancel plumbing |
| `terminal_create` PTY output (`terminal/mod.rs`) | raw PTY bytes (base64) | `AppHandle::emit` (`terminal:output:<id>`) | drop the terminal (`close`) | **unbounded, high-frequency** — needs its own coalescing pass, last slice |

## Recommended slices

Ordered so each lands independently and the seam tightens monotonically:

1. **Land the primitives + pilot the simplest egress.** Add
   `arbor-ipc::prelude::Stream` (envelope + lifecycle + `seq`) and FE
   `startStream`. Migrate `mr_start_conflict_resolution` to a broker handler on
   `Stream` — it's pure egress with no cancellation, so it exercises the happy path
   end-to-end and **closes the `mr` domain** (last inline command there).

2. **Refactor the existing-shape commands + kill the `Channel`.** Move
   `get_workdir_diff_stream` and `start_shell_detection` onto `Stream` (mechanical —
   they already use `EventSink`), and **convert `get_file_blame_streaming` off
   `tauri::ipc::Channel`**: it becomes `id + events`, progress ticks emit as
   `chunk`, the assembled lines as the `done` payload. Delete `import { Channel }`
   from `src/lib/ipc/diff.ts` and collapse the diff store's four listeners onto
   `startStream`.

3. **Generalize cancellation.** Introduce `StreamRegistry` (`state.streams()`) +
   the single `cancel_stream` handler, retire `cloud_cancellations` /
   `cloud_cancel` / `cloud_is_cancelled`, and port `cloud_list_stream`,
   `cloud_search_stream`, and `cloud_download_many` onto the shared token. This is
   where the producer-polls-`CancelToken` contract gets its first real users.

4. **Terminal, last and on its own.** PTY output is unbounded and arrives in 4 KiB
   reads at keystroke/output frequency — it must not emit one event per read. It
   gets a dedicated **coalescing pass** (buffer + flush on a short interval or size
   threshold) before it rides the `Stream` envelope, and it carries the
   `terminal:closed` terminal signal. Keep it out of slices 1–3 so the common
   bounded case lands first without being held up by the coalescing design.

## Invariants to preserve across the migration

- **Topics and payloads stay byte-identical** when a command merely moves onto
  `Stream` without changing identity (diff-stream, shell-detection) — the FE
  contract is unchanged, only the producer is centralized.
- **One id per activity.** `stream_id == job_id` wherever a `JobInfo` exists.
- **Subscribe-before-invoke** on the FE: a synchronous `started` must not outrun
  the listener. `startStream` guarantees the ordering.
- **No `AppHandle` in handlers.** Everything goes through `EventSink` /
  `StreamRegistry` reached from `&AppState`, so the seam survives the eventual
  process split unchanged.
