<h1>Settings — Project</h1>

<h2>IDE Integration</h2>
<p>
  Configure which IDE Arbor uses when opening a worktree or repository folder.
  All settings are stored in <code>~/.config/arbor/config.toml</code>.
</p>

<h3>Default IDE</h3>
<p>
  The IDE used when no language-specific default applies. Shown as <em>Default</em> badge
  in the worktree context menu.
</p>

<h3>IDE by Language</h3>
<p>
  Override the default IDE for a specific project type. For example, set <strong>RustRover</strong>
  for Rust and <strong>WebStorm</strong> for Node.js — the correct IDE will be highlighted and
  pre-selected automatically when right-clicking a worktree.
</p>
<p>
  Supported project types: Rust, Node.js, Java (Maven), Java (Gradle), Go, Python, .NET, C++, Ruby, PHP.
  Leave a language row set to <em>— Use default —</em> to fall back to the global Default IDE.
</p>

<h3>Executable Paths</h3>
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

<h3>Custom IDEs</h3>
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

<h3>IDE Detection</h3>
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

<h2>Terminals</h2>
<p>
  Configure the integrated terminal panel: which shells appear in the <strong>+</strong> picker,
  which one opens by default, and where each executable lives. Settings are stored under
  <code>[terminals]</code> in <code>~/.config/arbor/config.toml</code>.
</p>

<h3>Default Shell</h3>
<p>
  The shell opened by the bare <strong>+</strong> button in the terminal panel. Leave on
  <em>— platform default —</em> to fall back to <code>cmd.exe</code> on Windows and
  <code>bash</code> on Linux/macOS. Any built-in or custom shell can be set as default by
  clicking the check icon on its row.
</p>

<h3>Detected Shells</h3>
<p>
  Each built-in shell shows a status dot:
</p>
<ul>
  <li><strong>Green</strong> — found in <code>PATH</code> or at a well-known install location.</li>
  <li><strong>Grey ✕</strong> — not detected. The shell is hidden from the terminal picker.</li>
  <li><strong>Grey dot</strong> — detection has not finished running yet.</li>
</ul>
<p>
  Use the <strong>path override</strong> field if a shell is installed in a non-standard location,
  or to pin a specific version when several are available. Click the folder icon to browse, or
  paste the absolute path. Saving the form re-runs detection automatically when paths change.
</p>

<h3>Custom Terminals</h3>
<p>
  Register any executable as a terminal entry. Each custom shell needs:
</p>
<ul>
  <li><strong>ID</strong> — unique identifier (e.g. <code>dev-container</code>).</li>
  <li><strong>Display name</strong> — label shown in the picker.</li>
  <li><strong>Command</strong> — executable to launch (absolute path or <code>PATH</code>-resolvable name).</li>
  <li><strong>Args</strong> — optional arguments passed on spawn (space-separated).</li>
</ul>
<p>
  Custom terminals are <em>always</em> shown in the picker (they don't go through the detection
  probe — you defined them on purpose) and can be set as the default shell.
</p>

<h3>Shell Detection</h3>
<p>
  At startup Arbor probes each built-in shell in the background via <code>which</code> /
  <code>where</code>, with fallback paths for shells that don't add themselves to <code>PATH</code>
  (Git Bash, WSL, MSYS2, Cygwin, PowerShell 7). Detection runs as a non-cancellable background
  job — see <strong>System → Shell Detection</strong> in the Jobs overlay.
</p>
<ul>
  <li>Detection runs <strong>once per session</strong>. Use <strong>Re-detect</strong> after
      installing a new shell to refresh without restarting.</li>
  <li>Shells with an explicit path override are checked directly (file existence) rather than via
      <code>which</code>.</li>
  <li>Platform-irrelevant shells are filtered out: <code>cmd</code>/<code>powershell</code>/
      <code>wsl</code> never appear on Linux, <code>zsh</code>/<code>tcsh</code>/<code>sh</code>
      never appear on Windows.</li>
</ul>

<h2>Repository</h2>
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

<h2>Issue Tracker</h2>
<p>Per-project issue tracker and ticket link settings. All values are stored in <code>.arbor/config.toml</code>.</p>

<h3>Provider</h3>
<p>
  Select which issue tracker (Linear or Jira) to use for this repository's Issues sidebar and Ticket Picker.
  Changing this also resets the default project filter. Connect credentials first in <strong>Access → Issue Trackers</strong>.
</p>

<h3>Default Project Filter</h3>
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

<h3>Ticket Links — Custom Pattern</h3>
<p>
  Override the default ticket ID regex for this repository. Leave blank to use the tracker default
  (<code>[A-Z][A-Z0-9]*-\d+</code> for Linear/Jira, <code>#\d+</code> for GitHub/GitLab).
  The pattern must contain exactly one capture group, e.g. <code>\b(MYCO-\d+)\b</code>.
  See the <strong>Ticket Links</strong> section for full documentation.
</p>
