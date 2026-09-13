<script lang="ts">
  import Callout from '$lib/components/shared/ui/Callout.svelte';
</script>

<h1>Settings — Performance</h1>

<h2>Cache</h2>
<p>
  The cache stores each tab's graph, branch, CI/CD, and MR data in memory for the duration of the
  session. Switching to a tab whose data is already cached is <strong>instant</strong> — no round-trip
  to the backend is needed. Data is cleared when you close the app.
</p>
<ul>
  <li><strong>Enable cache</strong> — master toggle. When off every tab switch re-fetches data from the backend. Useful for debugging.</li>
  <li>
    <strong>Max cached tabs</strong> — maximum number of tabs whose snapshots are kept simultaneously.
    When exceeded, the least-recently-used tab's snapshot is evicted (LRU). Default: 10.
  </li>
  <li>
    <strong>Clear all</strong> — discards every in-memory snapshot and commit-detail cache immediately,
    and evicts the backend stats and ticket-link caches for every tab. The next access
    re-fetches from the backend and repopulates the cache.
  </li>
</ul>

<h4>What is cached</h4>
<ul>
  <li>Commit graph (page 0)</li>
  <li>Local and remote branches, stashes, tags, submodules, nearest tag</li>
  <li>CI/CD provider info and run list</li>
  <li>Plugin pipeline definitions and runs</li>
  <li>Open MR/PR list</li>
  <li>Squash-merge ghost-edge hints</li>
  <li>Individual commit details (global cache by SHA — commits are immutable)</li>
</ul>

<h4>What is never cached</h4>
<ul>
  <li>Working-tree status (staged / unstaged files) — always fetched live</li>
  <li>File diffs — always fetched live (see <em>Lazy commit diffs</em> below)</li>
  <li>Issue tracker / ticket data</li>
  <li>Paginated graph pages beyond page 0 ("Load more")</li>
  <li>Graph loads with a file filter active</li>
</ul>

<h4>Lazy commit diffs</h4>
<p>
  When you click a commit in the graph (or pick a stash), Arbor fetches only the <strong>file list</strong> with
  +/− stats first, then loads each file's hunks <strong>on demand</strong> as you open it in the diff viewer.
  Files you never click are never parsed. This keeps clicking a large commit responsive even when
  <em>Show full file</em> is on, because libgit2 only walks the bytes of files you actually look at.
</p>
<p>
  Inside the visible diff, files with hunks not yet loaded show a small <em>Parsing…</em> badge in the
  file list. Selecting one queues its parse; clicking another commit before it returns discards the
  in-flight fetch so stale hunks never overwrite the new file list. The loaded hunks are kept in memory
  only for the currently selected commit — switching commits re-fetches metadata and parses on demand
  again.
</p>

<h4>Cache invalidation</h4>
<p>
  The cache for a tab is discarded automatically after any write operation on that tab:
  committing, staging, discarding, checking out a branch, pushing, pulling, fetching,
  resetting, cherry-picking, rebasing, creating/deleting branches or tags, GitFlow operations,
  MR/PR mutations, and CI pipeline triggers.
</p>
<p>
  The status bar shows a <strong>last refreshed</strong> timestamp (e.g. <em>2m ago</em>) next to
  the branch name, indicating when the cached data was last fetched from the backend.
</p>

<h2>Process Monitor</h2>
<p>
  Arbor is not one process. It is the shell you are looking at, its interface, one backend per
  product, whatever those backends started — a language server, a JVM under test, a
  <code>cargo</code> build — and whatever <em>those</em> started. <strong>Show Process Monitor</strong>
  in the command palette opens one screen for all of it, from any window, with totals against what
  this machine has and three tables, because they answer three different questions:
</p>
<ul>
  <li><strong>Arbor</strong> — the application itself: its frontend and one backend per product, each
    under the product's own name and icon. This is the table to read to judge Arbor.</li>
  <li><strong>Language servers</strong> — started by a product for the projects open in it, with the
    icon of the product that started them. The usual reason a machine is slow, and the only rows with
    an action.</li>
  <li><strong>Everything else</strong> — what was started through Arbor: runs, builds, tests, a
    terminal's shell. It ends when you stop it.</li>
</ul>
<p>
  Bennu keeps loaded only what is in use. A project's index and language servers start when it is
  on screen; one that has gone ten minutes without being on screen or asked about — by you or by an
  AI client — gives back its index, framework models and library data, and its language servers
  stop (<em>Settings › Language Servers</em> sets the time). A project that leaves the workspace —
  closed, replaced, or left behind by a workspace switch — goes about twenty seconds later without
  waiting: a workspace switch reopens its projects one at a time, and releasing at once would close
  the very projects it is about to open again. A backend's row shrinks with what you stop using,
  rather than growing with every project opened in the session.
</p>
<h3>What a backend is holding</h3>
<p>
  A backend's row in the <strong>Arbor</strong> table opens into a breakdown of its own memory, per
  project. It is loaded when you open it — never on the one-second tick — and has its own
  <em>Refresh</em>. What each backend lists is what grows in it:
</p>
<ul>
  <li><strong>Bennu</strong> — per project, the sources kept as text, the reference index, the
    classes decoded from the JDK and from libraries, the index files; each framework model it built
    (Bevy's declarations, fulcrum's labels, XML schemas, the Maven repository); the files it keeps
    open for a language server with their diagnostics; the shader library; the crates Cargo
    completion offers.</li>
  <li><strong>Garrulus</strong> — the open vault's note text, its word index, titles and properties,
    links and unlinked mentions.</li>
  <li><strong>Picus</strong> — the scripts of each repository read, and the schema each connection
    reported, by connection name.</li>
  <li><strong>Merula</strong> — the samples the live session has decoded and keeps for playback.</li>
  <li><strong>Tyto</strong> — while recording, the newest frame and the most the encoder can have
    queued.</li>
  <li><strong>Corvus</strong> — per repository, its statistics and the ticket links looked up; the
    commit avatars resolved.</li>
  <li><strong>File Explorer</strong> — the git status behind the badges, per repository browsed.</li>
  <li>Every backend that hosts plugins lists each plugin's Lua memory — an exact figure, since Lua
    counts what it allocates. A busy plugin host says so; <em>Refresh</em> asks again.</li>
</ul>
<ul>
  <li>The figures are <strong>estimates</strong>, marked <code>≈</code>: a backend sizes its
    structures by walking them, since nothing keeps allocation statistics per category. Text held is
    summed and shown without the mark.</li>
  <li>A line reading <em>counted</em> gives a number of entries instead of bytes — the structure is
    too deep to size without doing more work than the answer is worth.</li>
  <li><em>mapped</em> marks memory-mapped index files. They are in the measured total, but the system
    can take those pages back whenever it needs them.</li>
  <li><em>Measured</em> against <em>Accounted for</em> shows the gap: the allocator's own overhead,
    and whatever is not sized yet.</li>
</ul>
<p>
  Language servers are not in a backend's breakdown: they are processes of their own, with a row and
  a real measurement in the <strong>Language servers</strong> table.
</p>
<p>
  Memory is the figure the system's own monitor leads with: the <strong>physical footprint</strong>
  on macOS — Activity Monitor's Memory column — and private bytes on Windows. Not resident size,
  which also counts pages of the program and its libraries that the system can drop at any time and
  shares with other processes; for a backend that can be a third again of the real figure.
</p>
<p>
  CPU is given as a percentage of <strong>one</strong> core, so a four-core machine can legitimately
  total 400 — the header says what share of the whole machine that is. The first reading after the
  screen opens shows <code>—</code> rather than 0%: CPU usage is the work done between two samples,
  and there has only been one.
</p>
<ul>
  <li><strong>Warn above … MB</strong> — a process holding more than this is marked. Off at 0.</li>
  <li><strong>… % CPU</strong> — marked when it holds more than this share of one core across
    <em>two consecutive</em> readings. One is noise: every language server pins a core while it
    indexes, and that is the system working rather than the system stuck.</li>
  <li><strong>Restart</strong> — offered on a language server, the one kind of process that can be
    restarted without losing anything, since it rebuilds its state from the files.</li>
</ul>
<Callout variant="important" title="It reports; it does not cap">
  There is no <code>-Xmx</code> for Arbor, and there cannot be a portable one: holding a native
  process under a memory ceiling is a job object on Windows, a cgroup on Linux, and nothing at all
  on macOS. A “maximum RAM” switch would work on one platform and silently do nothing on the other
  two, which is worse than not offering it. The limits that <em>are</em> real are per-product and
  live with that product — Bennu's indexing and validation thread budgets, in its own settings.
</Callout>

<h2>Memory Management</h2>
<p>
  Controls whether evicting a tab's cache also frees the underlying git handle held by the backend.
</p>
<ul>
  <li>
    <strong>Free git handle on eviction</strong> — when enabled (default), dropping a tab's cache also
    releases the <code>git2::Repository</code> object. This frees libgit2's internal caches: pack-file
    indexes, loose-object cache, reference cache, and config cache. The repository is transparently
    re-opened the next time any command accesses that tab, with a small one-time latency (~50 ms).
    Disable this only if you notice lag when switching back to evicted tabs.
  </li>
</ul>

<h2>Auto-Refresh Scheduler</h2>
<p>
  The scheduler runs in the background and periodically checks whether the active repository has
  changed since the cache was last populated.
</p>
<ul>
  <li><strong>Enable scheduler</strong> — toggle the background checker on or off.</li>
  <li><strong>Check interval</strong> — how often the scheduler wakes up (seconds, minimum 5). Default: 60 s.</li>
  <li><strong>Focus-gated</strong> — the scheduler only runs while the app window is focused. If you switch away and come back, it resumes from where it left off.</li>
</ul>

<h4>Change detection</h4>
<p>
  On each tick, the scheduler calls <code>get_repo_fingerprint</code> — a lightweight command that reads the current HEAD SHA
  and all ref names from libgit2. Fingerprints are compared; when a change is detected the tab's cache is discarded and the
  graph reloads automatically.
</p>

<h2>Idle Cache Eviction</h2>
<p>
  Automatically frees memory by evicting the cache of background tabs that have not been accessed
  for a configurable amount of time. Useful when many repositories are open simultaneously for
  extended sessions.
</p>
<ul>
  <li><strong>Enable auto-eviction</strong> — off by default. When enabled, a background scheduler periodically scans all cached tabs and discards those that have been idle too long.</li>
  <li>
    <strong>Minimum tabs to keep</strong> — the N most-recently-used tabs are always kept in cache,
    regardless of idle time. The currently active tab counts toward this total. Default: 1 (active tab only).
    Set to 3 to always keep the active tab plus the 2 most recently visited ones.
  </li>
  <li><strong>Idle threshold</strong> — seconds of inactivity before a tab's cache is cleared (minimum 30, default 300 s / 5 min). The timer is reset every time you switch to a tab or its data is accessed.</li>
  <li><strong>Check interval</strong> — how often the eviction scheduler runs (minimum 10 s, default 60 s). A shorter interval means more responsive eviction at a negligible CPU cost.</li>
</ul>

<h4>Eviction scope</h4>
<p>When a tab is evicted all three layers are cleaned:</p>
<ul>
  <li><strong>Frontend</strong> — the in-memory <code>TabSnapshot</code> (graph, branches, CI, MR, pipeline data), the commit-detail cache, and the fingerprint baseline are removed.</li>
  <li><strong>Backend</strong> — the stats cache (<code>RepoStats</code> computation result) and the ticket-link cache for that tab are cleared.</li>
  <li><strong>git2 handle</strong> — the <code>Repository</code> object is dropped (if "Free git handle on eviction" is enabled), freeing libgit2 internal memory.</li>
</ul>

<h4>Protected tabs</h4>
<p>
  The <em>minimum tabs to keep</em> most-recently-used tabs are always excluded from eviction.
  Switching to a tab resets its idle timer and moves it to the top of the recency list immediately.
</p>

<h2>Repository Browser</h2>
<p>
  The Repository Browser ships with a separate, persistent cache layer because listing every
  repo for an account against the GitHub or GitLab API is slow on large accounts (200+ projects).
  Unlike the per-tab cache above, this cache lives in <code>localStorage</code> and survives
  app restarts.
</p>
<ul>
  <li>
    <strong>Cache TTL</strong> — how long a fetched repo list stays valid (seconds, default 600 = 10 min).
    Within the TTL, opening the modal returns the cached list without a network call. Past the TTL the
    cached list is still shown immediately and a fresh fetch runs in the background; the strip in the
    modal flips from <em>Cached</em> to <em>Updated</em> once it completes. Set to <code>0</code> to
    disable caching entirely.
  </li>
  <li>
    <strong>Clear repo browser cache</strong> — wipes the on-disk cache for every connected provider.
    The next open re-fetches from the API.
  </li>
</ul>

<h4>Backend pagination is now parallel</h4>
<p>
  The repo-listing backend was rewritten to fetch pages 2..N concurrently (it used to walk them
  sequentially). For 200+ repos that alone collapses the cold-load time from ~30s into a handful
  of seconds. GitLab's <code>statistics=true</code> flag was also dropped — it forced the API to
  compute repo size for every project, and the list view doesn't display sizes anyway.
</p>
