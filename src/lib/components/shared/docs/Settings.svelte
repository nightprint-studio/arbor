<script lang="ts">
  import Kbd from '$lib/components/shared/internal/Kbd.svelte';
</script>

<h1>Settings</h1>

<p class="doc-lead">Open Settings with <Kbd action="settings" /> or via the gear icon in the Activity Bar. Settings are organised into groups in the left sidebar — <em>Interface</em>, <em>Git</em>, <em>Tools</em>, <em>Performance</em>, <em>Access</em>, and <em>Project</em>. Global, host-wide tooling (pipeline cap, IDE registry, terminal registry) lives under <em>Tools</em>; per-repository overrides for those tools sit under <em>Project</em>.</p>

<h2>Interface</h2>

<h3>Appearance</h3>
<ul>
  <li><strong>Font scale</strong> — scales all UI text from 0.8× to 1.4× in 5 % increments. Useful on HiDPI or small screens.</li>
  <li><strong>Detail panel position</strong> — choose whether the commit detail / diff panel opens at the Bottom or as a Right sidebar.</li>
</ul>

<h3>Animations</h3>
<p>
  Controls the speed and behaviour of every transition and motion effect in the UI.
  Settings are stored in <code>localStorage</code> and take effect immediately.
</p>
<ul>
  <li>
    <strong>Enable animations</strong> — master toggle. When off, all transitions play at
    zero duration: panels appear instantly, toasts pop in without sliding, modals open without
    scaling. Useful for accessibility (reduced-motion preference) or low-powered hardware.
  </li>
  <li>
    <strong>Speed</strong> — three presets that scale every duration proportionally:
    <ul>
      <li><em>Snappy</em> — ~55 % of default durations. Tight, fast feel.</li>
      <li><em>Normal</em> — 100 % (default). Balanced and polished.</li>
      <li><em>Relaxed</em> — ~165 % of default durations. Smoother, more fluid motion.</li>
    </ul>
  </li>
  <li>
    <strong>Preview</strong> — hit <em>Replay</em> to see an animated chip using the current speed
    setting without leaving the panel.
  </li>
</ul>
<p>
  Animations that are controlled by this setting include: sidebar slide-in, bottom/right panel
  slide-in, modal and command-palette entrance, toast slide-in/out, overlay fade, settings section
  fade, and all CSS <code>transition</code> properties on interactive elements (hover states, toggles, buttons).
</p>

<h3>Graph</h3>
<ul>
  <li><strong>Commits per load</strong> — how many commits are fetched each time the graph loads or is scrolled to the end (100 – 2000, default 500). Only applies when lazy-load pagination is on.</li>
  <li><strong>Show remote branches</strong> — toggle remote-tracking refs (e.g. <code>origin/main</code>) in the lane graph.</li>
  <li><strong>Show tags</strong> — toggle annotated and lightweight tags in the lane graph.</li>
  <li><strong>Lazy-load pagination</strong> — when <em>on</em> (default), commits are loaded in batches as you scroll; when <em>off</em>, the entire repository history is loaded at once. Disable only on small repos — loading tens of thousands of commits at startup can be slow. Persisted to <code>~/.config/arbor/config.toml</code>.</li>
</ul>

<h3>Diff &amp; Stage</h3>
<ul>
  <li><strong>Context lines</strong> — number of unchanged lines shown around each hunk (0 – 20, default 3).</li>
  <li><strong>View mode</strong> — Unified (single column) or Split (side-by-side).</li>
  <li><strong>Confirm before discarding</strong> — when enabled (default), a confirmation dialog appears before discarding a single file's changes. The <em>Discard All</em> confirmation is always shown regardless of this setting.</li>
</ul>

<h3>Keybindings</h3>
<p>
  Click any shortcut chip to record a new key combination. Press <kbd>Escape</kbd> while recording to cancel.
  Use the reset icon to restore a single binding to its default. <strong>Reset all</strong> restores every binding at once.
</p>
<p>
  The <strong>Plugins</strong> group at the bottom of the list shows keybindings registered by plugins — these are read-only.
</p>

<h3>Activity Bar</h3>
<p>
  The Activity Bar can be customised without touching any setting panel.
  Click the <strong>gear icon → Customize Activity Bar…</strong> in the title bar to open the layout editor.
</p>
<ul>
  <li>
    <strong>Visibility</strong> — each item has an eye icon. Click it to show or hide the button.
    Items marked with a lock icon (<em>Branches</em>, <em>Stage</em>, <em>Detail</em>) are mandatory and cannot be hidden.
  </li>
  <li>
    <strong>Order</strong> — drag items by their handle to reorder them within their section.
    A blue indicator line shows the insertion point as you drag.
  </li>
  <li>
    <strong>Two sections</strong> — <em>Sidebar</em> (top half, controls which panel opens on the left)
    and <em>Panel</em> (bottom half, controls the bottom panel and right detail panel).
    Items can only be reordered within their own section.
  </li>
  <li>
    <strong>Plugin items</strong> — actions, combo buttons, and separators registered by plugins also
    appear here and can be reordered or hidden like built-in items.
  </li>
</ul>
<p>
  The layout is persisted to <code>~/.config/arbor/config.toml</code> and restored on next launch.
  Hidden items are still active in the background — for example, hiding the Stage button does not
  disable staging; it just removes the shortcut from the bar.
</p>

<h2>Git</h2>

<h3>Git Flow</h3>
<p>See the dedicated <strong>Git Flow</strong> section in the sidebar for full documentation.</p>

<h3>Experimental</h3>
<p>
  Features that depend on external data, are still maturing, or may produce unexpected results
  in edge cases. All flags default to <strong>on</strong> and are stored in
  <code>localStorage</code> — they never alter local Git state and can be toggled at any time.
</p>

<h4>Squash-merge ghost edges</h4>
<p>
  When a Pull Request or Merge Request is merged via <em>squash</em>, Git creates a single new
  commit on the target branch whose only parent is the previous tip — there is no topological link
  to the original feature commits. The commit graph therefore shows the feature branch as a dangling
  strand with no visible connection to the merge point.
</p>
<p>
  When this flag is on, Arbor queries the GitHub / GitLab API on every graph load to retrieve the
  <code>merge_commit_sha</code> of each closed PR / merged MR, then draws a dashed ghost edge
  connecting that commit to the feature branch tip.
</p>
<ul>
  <li><strong>Ghost edge style</strong> — semi-transparent dashed line (45 % opacity, <code>5 3</code> dash pattern) in the feature branch's lane colour.</li>
  <li><strong>Fallback anchor</strong> — if the merge commit hasn't been fetched locally yet, the ghost edge connects the feature tip to the target branch tip <em>before</em> the merge. Once you <code>git fetch</code>, native edges appear and the ghost is suppressed automatically.</li>
  <li><strong>No token / no remote</strong> — degrades silently; graph loads normally without ghost edges.</li>
  <li><strong>Performance</strong> — adds one API call per graph load (up to 50 closed PRs/MRs). May add latency on repos with many closed PRs.</li>
</ul>

<h2>Performance</h2>

<h3>Cache</h3>
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
  <li>File diffs — always fetched live</li>
  <li>Issue tracker / ticket data</li>
  <li>Paginated graph pages beyond page 0 ("Load more")</li>
  <li>Graph loads with a file filter active</li>
</ul>

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

<h3>Memory Management</h3>
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

<h3>Auto-Refresh Scheduler</h3>
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

<h3>Idle Cache Eviction</h3>
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
<p>
  When a tab is evicted all three layers are cleaned:
</p>
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

<h2>Access — Git &amp; Integrations</h2>

<p>
  The <strong>Git &amp; Integrations</strong> section consolidates Git host accounts, credentials, and issue tracker connections into a single place.
  All secrets are stored in the OS keychain (Windows Credential Manager, macOS Keychain, libsecret on Linux).
</p>

<h3>Git Providers (GitHub / GitLab)</h3>
<p>
  Each provider card has a split <strong>Connect</strong> button. Click the main button to connect via the default method (OAuth),
  or click the <strong>▾</strong> chevron to pick a different method:
</p>
<ul>
  <li><strong>OAuth (device flow)</strong> — RFC 8628 Device Authorization Grant. No redirect or local server needed.
      A code is shown; enter it at the provider's verification URL. Arbor polls until authorisation completes.
      The OAuth token enables CI/CD, PR/MR management, and remote repository creation.</li>
  <li><strong>Personal Access Token</strong> — paste a PAT directly. Stored in the keychain and used for HTTPS operations.</li>
  <li><strong>Username + Password</strong> — store a username and password/token pair. Used automatically for fetch, pull, and push.</li>
</ul>
<p>For self-hosted GitLab, check <strong>Self-hosted</strong> and enter your instance hostname before saving.</p>

<h3>Additional Git Credentials</h3>
<p>
  The <strong>Additional Git Credentials</strong> card lets you store credentials for other hosts
  (Bitbucket, Azure DevOps, custom Git servers). Select a provider preset or choose <em>Custom…</em> and enter the host manually.
</p>

<h3>Issue Trackers (Linear / Jira)</h3>
<p>Each tracker card uses the same split <strong>Connect</strong> button pattern — click the main button for the default method or <strong>▾</strong> for alternatives.</p>

<h4>Linear</h4>
<ul>
  <li>
    <strong>OAuth (recommended)</strong> — Authorization Code + PKCE with a localhost callback server on port 7729.
    Register a <em>Public</em> OAuth app at <code>linear.app → Settings → API → OAuth applications</code> and set
    <code>http://127.0.0.1:7729/callback</code> as the redirect URI.
    Paste the <strong>Client ID</strong> and click <strong>Authorize</strong> — Arbor opens the browser and completes the flow automatically.
  </li>
  <li>
    <strong>Personal API Key</strong> — generate at <code>linear.app → Settings → API → Personal API keys</code> and paste directly.
  </li>
</ul>

<h4>Jira</h4>
<ul>
  <li>
    <strong>API Token — Jira Cloud</strong> — generate at <code>id.atlassian.com → Security → API tokens</code>.
    Enter your subdomain (the part before <code>.atlassian.net</code>), email, and the token.
  </li>
  <li>
    <strong>Personal Access Token — Jira Data Center / Server</strong> — generate at <code>Jira → Profile → Personal Access Tokens</code>.
    Enter the full hostname as subdomain (e.g. <code>jira.internal.example.com</code>), your email, and the PAT.
    Self-signed and internal-CA certificates are accepted automatically.
  </li>
  <li>
    <strong>OAuth 2.0 (3LO) — Jira Cloud only</strong> — click <strong>Connect ▾ → OAuth 2.0</strong> and follow the browser prompt.
    Arbor discovers your Cloud site automatically and stores access + refresh tokens in the OS keychain.
  </li>
</ul>
<p>See the <strong>Issues (Linear / Jira)</strong> section for the full compatibility table, sidebar filters, and plugin API.</p>

<h2>Tools</h2>

<p>
  Global, host-wide tooling shared by every repository. Per-project overrides for these
  tools live under the <strong>Project</strong> group below.
</p>

<h3>Pipelines</h3>
<p>
  Tunes the local pipeline orchestrator used by plugins (<code>arbor.pipeline.define</code> /
  <code>arbor.pipeline.run</code>). CI/CD runs on GitHub Actions or GitLab CI are scheduled
  by the provider and are <strong>not</strong> affected by these settings.
</p>
<ul>
  <li>
    <strong>Max concurrent runs</strong> — cap on the number of pipeline runs that may
    execute at the same time across all plugins. Additional runs queue up with a
    <em>Queued</em> badge in the Pipelines panel and start as soon as a slot frees up.
    Default: <code>4</code>.
  </li>
  <li>
    <strong><code>0</code> means unlimited</strong> — the orchestrator never queues; every
    run starts immediately. Useful for quick experiments, but a burst of pipelines
    (sequence runs, group dashboards, large fan-outs) can saturate disk I/O, libgit2
    packfile readers, and the network — leading to noticeable slowdowns and a jittery UI.
    Keep a cap unless you specifically need the parallelism.
  </li>
  <li>
    Changes apply within ~250 ms — no app restart needed. Saving wakes any orchestrator
    parked on the global condvar so newly-allowed slots are claimed in the next tick.
  </li>
</ul>
<p>
  Stored in <code>~/.config/arbor/config.toml</code> under <code>[pipelines]</code>.
  See the dedicated <strong>Pipelines</strong> docs page for the full lifecycle and
  the <code>silent</code> flag.
</p>

<h3>IDE Integration</h3>
<p>
  Configure which IDE Arbor uses when opening a worktree or repository folder.
  All settings are stored in <code>~/.config/arbor/config.toml</code>. Per-repository
  override (<em>"open this project with this IDE"</em>) lives under
  <strong>Project → External Integrations</strong>.
</p>

<h4>Default IDE</h4>
<p>
  The IDE used when no language-specific default applies. Shown as <em>Default</em> badge
  in the worktree context menu.
</p>

<h4>IDE by Language</h4>
<p>
  Override the default IDE for a specific project type. For example, set <strong>RustRover</strong>
  for Rust and <strong>WebStorm</strong> for Node.js — the correct IDE will be highlighted and
  pre-selected automatically when right-clicking a worktree.
</p>
<p>
  Supported project types: Rust, Node.js, Java (Maven), Java (Gradle), Go, Python, .NET, C++, Ruby, PHP.
  Leave a language row set to <em>— Use default —</em> to fall back to the global Default IDE.
</p>

<h4>Executable Paths</h4>
<p>
  Each built-in IDE entry shows a status dot (green = detected, grey = not found) and an optional
  path override field. Use the override if:
</p>
<ul>
  <li>The IDE executable is not in <code>PATH</code> (common on Windows for JetBrains IDEs).</li>
  <li>You have multiple versions installed and want to pin a specific one.</li>
  <li>The default command name doesn't match your installation (e.g. a custom build).</li>
</ul>
<p>Click the folder icon to browse for the executable, or type the path directly. Changes apply after <strong>Save</strong>.</p>

<h4>Custom IDEs</h4>
<p>
  Register any editor not in the built-in list. Each custom entry requires:
</p>
<ul>
  <li><strong>ID</strong> — unique identifier (e.g. <code>emacs</code>). Used internally.</li>
  <li><strong>Name</strong> — display name shown in menus.</li>
  <li><strong>Command</strong> — executable to launch (absolute path or <code>PATH</code>-resolvable name).</li>
  <li><strong>Args</strong> — optional extra arguments passed before the target path (space-separated).</li>
</ul>
<p>Custom IDEs appear in the worktree context menu alongside built-in ones and can be set as the default.</p>

<h4>IDE Detection</h4>
<p>
  At startup, Arbor probes each built-in IDE in the background via <code>which</code> / <code>where</code>.
  This runs as a non-cancellable background job (<strong>System → IDE Detection</strong>) so it never
  blocks the UI. Results populate the Executable Paths status dots and the worktree context menu.
</p>
<ul>
  <li>Detection runs <strong>once per session</strong>. Closing and reopening Settings does not re-trigger it.</li>
  <li>Click <strong>Re-detect</strong> (or press <strong>Save</strong>) to run a new detection pass — useful after installing an IDE mid-session or changing a path override.</li>
  <li>IDEs with an explicit path override are checked directly (file existence) — no <code>which</code> call needed.</li>
</ul>

<h3>Terminals</h3>
<p>
  Registers the shell entries available in the integrated terminal panel (built-in
  shells + user-defined custom terminals + per-shell executable path overrides). The
  per-shell preferences here are global — there is no per-project terminal override at
  the moment.
</p>
<ul>
  <li><strong>Default shell</strong> — opened by the bare <code>+</code> button in the terminal tabs. <em>None</em> ⇒ platform default.</li>
  <li><strong>Custom terminals</strong> — free-form executable + args entries that always show up in the picker.</li>
  <li><strong>Path overrides</strong> — pin an absolute path for a built-in shell when the binary isn't on <code>PATH</code>.</li>
</ul>

<h2>Project</h2>

<p>
  Per-repository settings stored alongside the repo in <code>.arbor/config.toml</code>.
  Requires an open repository — the page shows an empty state otherwise.
</p>

<h3>External Integrations</h3>
<p>
  Project-bound override for <strong>Tools → IDE Integration</strong>. Pick which IDE
  is launched when "Open in IDE" actions on this repo don't pin a specific target.
</p>
<ul>
  <li>
    <strong>IDE</strong> — choose any built-in IDE or custom IDE registered globally.
    The dropdown's leading <em>— Use global default —</em> entry clears the override
    and falls back to the global default IDE.
  </li>
  <li>
    Stored as <code>ide_id</code> in the repository's <code>.arbor/config.toml</code>.
    The <strong>Reset to global</strong> button removes the override entirely.
  </li>
  <li>
    Manage the list of installed IDEs and detection under
    <strong>Tools → IDE Integration</strong>.
  </li>
</ul>

<h3>Git Flow</h3>
<p>
  Project-bound override for <strong>Git → Git Flow</strong>. Toggle <em>Enable
  project override</em> to start customising Git Flow finish-action defaults
  (force PR/MR, default branch action, ticket-branch enforcement) for this
  repository only.
</p>
<ul>
  <li>The form is seeded from the current global defaults the first time you enable the override.</li>
  <li><strong>Save Project Override</strong> writes to <code>.arbor/config.toml</code> in the repo root.</li>
  <li><strong>Reset to global</strong> removes the override and falls back to the global Git Flow defaults.</li>
</ul>

<h3>Repository</h3>
<p>Per-project overrides stored in <code>.arbor/config.toml</code> alongside the repository. Requires an open repository to edit.</p>
<ul>
  <li>
    <strong>Commit template</strong> — pre-fills the commit message field when empty. Git's native
    <code>commit.template</code> takes priority if set; otherwise this template is used.
    Stored in <code>~/.config/arbor/config.toml</code> (applies to all repos).
  </li>
  <li><strong>Display name</strong> — friendly name shown in the tab bar instead of the folder name.</li>
  <li><strong>Default remote</strong> — remote used for fetch/pull/push when not specified. Defaults to <code>origin</code>.</li>
  <li><strong>Author identity override</strong> — sets <code>user.name</code> / <code>user.email</code> for commits in this repo only. Leave blank to use the global Git identity.</li>
</ul>

<h3>Issue Tracker</h3>
<p>Per-project issue tracker and ticket link settings. All values are stored in <code>.arbor/config.toml</code>.</p>

<h4>Provider</h4>
<p>
  Select which issue tracker (Linear or Jira) to use for this repository's Issues sidebar and Ticket Picker.
  Changing this also resets the default project filter. Connect credentials first in <strong>Access → Issue Trackers</strong>.
</p>

<h4>Default Project Filter</h4>
<p>
  When set, the Issues sidebar and Ticket Picker automatically pre-filter issues to the chosen project every time this
  repository is active. The user can still override the project from the filter bar at any time.
</p>
<ul>
  <li>Click the project combobox to see all projects available in the connected tracker.</li>
  <li>Select <em>— All projects —</em> to clear the default filter.</li>
  <li>Use the <strong>↺</strong> refresh button to reload the project list from the tracker.</li>
  <li>The selected project ID is stored as <code>issue_tracker_project_id</code> in <code>.arbor/config.toml</code>.</li>
</ul>

<h4>Ticket Links — Custom Pattern</h4>
<p>
  Override the default ticket ID regex for this repository. Leave blank to use the tracker default
  (<code>[A-Z][A-Z0-9]*-\d+</code> for Linear/Jira, <code>#\d+</code> for GitHub/GitLab).
  The pattern must contain exactly one capture group, e.g. <code>\b(MYCO-\d+)\b</code>.
  See the <strong>Ticket Links</strong> section for full documentation.
</p>

