# bennu-live

Live runs for Bennu — a LEAF crate (std only: threads and `TcpStream`; no async runtime, no Tauri, no IPC).

| Module | What it is |
|---|---|
| `process` | `Manager`: start a shell command (`cmd /C` / `sh -c`), id back at once, ring-buffered timestamped stdout/stderr, readiness (`wait_ready`), polite-then-tree stop, caps (running, retained, lifetime, log size), recipes (`start_recipe` / `stop_recipe`) |
| `tree` | `own_group` / `terminate_tree` / `kill_tree` — the cross-platform process-tree kill (`taskkill /T` / `killpg`); `bennu-be`'s `child.rs` delegates here |
| `logbuf` | `LogBuffer`: bounded, sequenced, queryable (`since`, `tail`, regex, stream) |
| `ready` | `Check` / `Readiness`: port open, log regex, loopback health URL, delay |
| `recipe` | recipes file (TOML) parse + validation, content hash for approvals, `resolve_cwd` |
| `http` | loopback-only HTTP/1.1 client, `X-Correlation-Id`, header redaction |
| `timeline` | the unified event stream, `merge`, `trace` (a correlation id across processes, with stack continuation lines) |
| `scenario` | scenario file (TOML), assertions, `run_scenario` over a `World` trait (testable with a fake) |
| `jsonpath` | the `$.a.b[0]` subset assertions use |
| `queue` | `Queue`: numbered tickets per build target, strictly fair FIFO, coalescing of same-class requests queued before a run starts, cancel / timeout, observer and snapshot for the panel |
| `approval` | in-memory approvals bound to a recipes-file hash |
| `logrecord` | lines -> records (a line with its stack trace / backtrace / multi-line SQL), stable ids; levels, times and exceptions per line come from `arbor-logscan`, plus JSON and logfmt logs |
| `logread` | filtering records by time / level / pattern with context, folding repeats, the per-level summary |
| `logfacets` | the digest: exceptions (class, root cause, first project frame), queries, requests, warnings; the `where` filter |
| `logsql` / `logaccess` | finding and normalising SQL statements and HTTP requests in log lines |
| `logtime` / `logfile` | ISO-8601 / epoch / `5m` parsing and the incremental read plan of a log file (offset, rotation) |

Everything public is exported through `prelude`. Files live under `<project>/.arbor/bennu/live/`
(`recipes.toml`, `scenarios/*.toml`); the formats are documented in the module docs of `recipe` and `scenario`
and in the in-app docs page *Live services*.
