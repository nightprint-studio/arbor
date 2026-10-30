# bennu-profile

Pure core of Bennu's profiler. Bytes and text in, a compact aggregated profile out — it spawns nothing and knows no Tauri; the backend (`bennu-be`, `profiler` domain) does the spawning.

| Module | What |
|---|---|
| `model` | `Profile`, `CallTree` (arena nodes, interned frames), per-method / allocation / lock / GC tables |
| `builder` | `TreeBuilder` (streaming, node-capped), method stats without recursion double-counting, nested `FlameNode` |
| `jfr_detect` | `JdkFacts` + `classify` — can this JDK record, and how is the file read (the matrix is in the module docs) |
| `jfr_events` | `EventSplitter` for `jfr print --json`, `JfrAggregator`, the Java-helper line format; also the off-CPU waits (sockets, files, parks) and the allocation stacks, kept as two more trees |
| `jfr_helper` | the embedded `ArborJfrDump.java` reader for JDKs with no `jfr` tool |
| `gzip` | `read_maybe_gzip` — plain or gzip (by magic bytes) with a decoded-size cap, for samply's `.json.gz` |
| `cpuprofile` | V8 `.cpuprofile` → tree |
| `firefox` | samply's Firefox-profiler JSON → tree (adapter, fixture-tested only) |
| `metrics` | per-file method and line metrics for the editor, matched by qualified name: CPU, allocation credited to the nearest project method, waits, and which methods the recording never caught |
| `alloc_code` | the allocation tree walked up to the project's own methods — who allocates, by type |
| `compare` | change in each method's share between two recordings |
| `target` | `ProfileTarget` — how a process is started under / attached to a profiler |

Everything public is exported through `prelude`.
