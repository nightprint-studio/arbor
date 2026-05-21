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
