<h1>Worktrees</h1>

<p class="doc-lead">
  Worktrees are Git <em>linked worktrees</em> — additional checked-out working directories that
  share the same repository. Each worktree has its own branch, working tree, and HEAD commit,
  letting you switch contexts instantly without stashing or committing.
</p>

<div class="callout tip">
  <strong>Fast switch</strong>
  Double-click any worktree row in the sidebar to open it as a new tab immediately.
</div>

<h2>Sidebar panel</h2>
<p>
  Expand the <strong>Worktrees</strong> section in the left sidebar (Layers icon).
  Each row shows the project-type emoji, branch name, and status badges:
</p>
<table class="shortcuts-table">
  <thead><tr><th>Badge</th><th>Meaning</th></tr></thead>
  <tbody>
    <tr><td>🏠 <em>Home</em></td><td>Main worktree — the directory where <code>.git/</code> lives. Cannot be removed.</td></tr>
    <tr><td>⊙ <em>CircleDot</em></td><td>Currently open in the active tab.</td></tr>
    <tr><td>🔒 <em>Lock</em></td><td>Locked via <code>git worktree lock</code> — cannot be pruned accidentally.</td></tr>
  </tbody>
</table>

<h2>Adding a worktree</h2>
<ol class="step-list">
  <li>Click the <strong>+</strong> button in the Worktrees header.</li>
  <li>Choose a destination folder (folder picker dialog).</li>
  <li>Select an existing branch <em>or</em> enable <strong>Create new branch</strong> and type a name.</li>
  <li>Click <strong>Add Worktree</strong> — Git creates the linked worktree immediately.</li>
</ol>

<h2>Switching worktrees</h2>
<ul>
  <li><strong>Double-click</strong> a row — opens the worktree path as a new tab (equivalent to <em>Open Recent</em>).</li>
  <li><strong>Right-click → Switch to this worktree</strong> — same action from the context menu.</li>
  <li><strong>ⓘ Info modal → Switch here</strong> — switches from the info overlay.</li>
</ul>

<h2>Right-click context menu</h2>
<p>Right-click any worktree row to see:</p>
<ul>
  <li><strong>Switch to this worktree</strong> — opens the worktree in a new tab (only visible when not current).</li>
  <li><strong>Worktree info</strong> — opens the info modal.</li>
  <li><strong>Open in IDE</strong> — sub-section listing every IDE detected on the system plus any custom IDEs.
      The IDE that matches the project-language default (or the global default) shows a <em>Default</em> badge.</li>
  <li><strong>Remove worktree</strong> — runs <code>git worktree remove</code>. Only visible for non-main worktrees.
      Locked worktrees cannot be removed without unlocking them first.</li>
</ul>

<h2>Info modal</h2>
<p>Click the ⓘ button on any row, or use the context menu. The modal shows:</p>
<div class="feature-grid two-col">
  <div class="feature-card">
    <div class="fc-title">Details</div>
    <div class="fc-desc">Full path, branch name, HEAD commit SHA, and detected project type (Rust, Node.js, Java Maven/Gradle, Go, Python, .NET, C++, Ruby, PHP).</div>
  </div>
  <div class="feature-card">
    <div class="fc-title">Sync Status</div>
    <div class="fc-desc">Blue <strong>↑ N ahead</strong> chip and orange <strong>↓ N behind</strong> chip relative to the remote upstream. Green <em>Up to date</em> when in sync. Purple <strong>N changes</strong> chip for local modifications; green <em>Clean</em> when the working tree is untouched.</div>
  </div>
</div>
<p>The action bar at the bottom of the modal offers <strong>Switch here</strong> and <strong>Open in IDE</strong> buttons.</p>

<h2>Project-type detection</h2>
<p>
  Arbor inspects each worktree directory for build-system markers to assign a project type.
  The emoji displayed in the sidebar reflects the detected type:
</p>
<table class="shortcuts-table">
  <thead><tr><th>Emoji</th><th>Type</th><th>Detected by</th></tr></thead>
  <tbody>
    <tr><td>🦀</td><td>Rust</td><td><code>Cargo.toml</code></td></tr>
    <tr><td>🟩</td><td>Node.js</td><td><code>package.json</code></td></tr>
    <tr><td>☕</td><td>Java (Maven)</td><td><code>pom.xml</code></td></tr>
    <tr><td>☕</td><td>Java (Gradle)</td><td><code>build.gradle</code> / <code>build.gradle.kts</code></td></tr>
    <tr><td>🐹</td><td>Go</td><td><code>go.mod</code></td></tr>
    <tr><td>🐍</td><td>Python</td><td><code>pyproject.toml</code>, <code>setup.py</code>, or <code>requirements.txt</code></td></tr>
    <tr><td>🔷</td><td>.NET</td><td><code>*.csproj</code> or <code>*.sln</code></td></tr>
    <tr><td>⚙️</td><td>C++</td><td><code>CMakeLists.txt</code> or <code>Makefile</code></td></tr>
    <tr><td>💎</td><td>Ruby</td><td><code>Gemfile</code></td></tr>
    <tr><td>🐘</td><td>PHP</td><td><code>composer.json</code></td></tr>
  </tbody>
</table>

<h2>IDE integration</h2>
<p>
  Each worktree can be opened directly in any IDE that Arbor has detected on the system.
  Configure IDE preferences in <strong>Settings → Project → IDE Integration</strong>.
</p>
<ul>
  <li>The <strong>default IDE per language</strong> setting means a Rust project opens in RustRover (or whichever IDE you chose for Rust) while a Node.js project opens in WebStorm — automatically, via the same "Open in IDE" menu entry.</li>
  <li>On Windows, IDEs that ship as batch scripts (<code>code.cmd</code>, <code>cursor.cmd</code>, etc.) are launched correctly through <code>cmd /c</code> — no manual workaround needed.</li>
</ul>

<div class="callout info">
  <strong>Git worktrees vs. branches</strong>
  A worktree is not a clone — it shares the full Git history and object store with the main
  repository. Disk usage is minimal (only the working tree files are duplicated). You can have
  multiple branches checked out simultaneously without any stashing.
</div>
