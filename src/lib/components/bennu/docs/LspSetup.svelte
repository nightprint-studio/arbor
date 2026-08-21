<!-- Bennu docs — installing a language server, where it is found, and which root it serves. -->
<h1>Installing a language server</h1>
<p class="doc-lead">
  Which servers Bennu knows about, how to install one from inside the app, where it looks for the
  binary, and why the project root decides what a server can see.
</p>

<h2>Supported out of the box</h2>
<table>
  <thead><tr><th>Language</th><th>Server</th><th>Files</th><th>Project marker</th></tr></thead>
  <tbody>
    <tr><td>Rust</td><td><code>rust-analyzer</code></td><td><code>.rs</code></td><td><code>Cargo.toml</code></td></tr>
    <tr><td>Go</td><td><code>gopls</code></td><td><code>.go</code></td><td><code>go.mod</code></td></tr>
    <tr><td>Python</td><td>Pyright</td><td><code>.py</code></td><td><code>pyproject.toml</code>, <code>setup.py</code></td></tr>
    <tr><td>C / C++</td><td><code>clangd</code></td><td><code>.c</code>, <code>.cpp</code>, <code>.h</code>, …</td><td><code>compile_commands.json</code>, <code>CMakeLists.txt</code></td></tr>
    <tr><td>TypeScript / JavaScript</td><td><code>typescript-language-server</code></td><td><code>.ts</code>, <code>.tsx</code>, <code>.js</code>, …</td><td><code>tsconfig.json</code>, <code>package.json</code></td></tr>
    <tr><td>Svelte</td><td><code>svelteserver</code></td><td><code>.svelte</code></td><td><code>svelte.config.js</code>, <code>package.json</code></td></tr>
    <tr><td>Angular (templates)</td><td><code>ngserver</code></td><td><code>.html</code></td><td><code>angular.json</code></td></tr>
    <tr><td>Lua</td><td><code>lua-language-server</code></td><td><code>.lua</code></td><td><code>.luarc.json</code>, <code>plugin.toml</code></td></tr>
    <tr><td>WGSL</td><td><code>wgsl-analyzer</code></td><td><code>.wgsl</code></td><td><code>Cargo.toml</code>, <code>.git</code></td></tr>
  </tbody>
</table>
<p>
  Settings → Language Servers lists each one with the path it resolved to, or — when it found
  nothing — how to install it. For the ones distributed through a package manager you already have
  (<code>rustup</code>, <code>cargo</code>, <code>go</code>, <code>npm</code>) there is an
  <strong>Install</strong> button beside the hint that runs exactly that command, streaming into the
  Build panel as it goes: <code>cargo install --git</code> builds a server from source and takes
  minutes, so it is worth watching rather than waiting on.
</p>
<p>
  It runs a command rather than downloading a binary, and that is deliberate: the server lands where
  the rest of your toolchain lives, so it keeps working after Arbor is updated or removed, and
  <code>wgsl-analyzer --version</code> says the same thing in your own terminal. The ones installed
  as a system package (<code>clangd</code> is LLVM, <code>lua-language-server</code> is Homebrew)
  have no button — Bennu installs language servers, not toolchains.
</p>
<p>
  <strong>WGSL works without one.</strong> A shader gets diagnostics, completion and find-usages
  whether or not <code>wgsl-analyzer</code> is installed — see <em>Shaders</em> under Editing. The
  server, when present, serves the file instead, which is worth knowing before installing it on a
  <strong>Bevy</strong> project: its module system is WESL rather than naga_oil, so it does not
  understand <code>#import</code>, and it replaces Bennu's deliberate silence on a composed shader
  with its own reading of those lines.
</p>
<h2>Svelte and Angular</h2>
<p>
  A <code>.svelte</code> file is coloured as markup with <code>&lt;script&gt;</code> and
  <code>&lt;style&gt;</code> in it, immediately and with no server. What that cannot know — that a
  name is a component, a prop, a store — arrives from <code>svelteserver</code> as semantic tokens
  layered on top, along with completion, hover, diagnostics, go-to, find-usages and rename. The
  template syntax itself (<code>&#123;#each&#125;</code>, <code>&#123;expr&#125;</code>) stays
  uncoloured: no HTML grammar can be told it is Svelte, and pretending otherwise would colour it
  wrongly rather than not at all.
</p>
<p>
  <strong>Angular serves templates, not <code>.ts</code>.</strong> Its server also speaks
  TypeScript, but one server serves a file here and the first match wins for <em>every</em>
  project — so claiming <code>.ts</code> would take every TypeScript file on the machine away from
  <code>typescript-language-server</code>, Angular project or not. Templates are the half that
  otherwise has nothing, so this is the trade that only adds. A <code>.ts</code> file keeps the
  server it has always had, which means a rename started in a component's class does not follow the
  name into its template.
</p>
<p>
  Wanting the other trade is a <code>[[lsp.servers]]</code> entry with
  <code>id = "angular"</code>: a custom entry shadows the built-in completely, so its
  <code>extensions</code> and its <code>args</code> are the ones used. Spell out the probe
  locations if you do — <code>ngserver</code> is a front end for the TypeScript and Angular
  language services and locates them from the command line; without them it starts, handshakes,
  and answers nothing.
</p>
<p>
  One difference worth knowing about, because it looks like a Bennu bug when it bites: a language
  server either watches the project for outside changes itself, or trusts the editor to tell it.
  Most of them watch for themselves, which is what Bennu wants — it does not deliver those
  notifications, so a server that stopped watching would quietly stop noticing a file
  <code>cargo build</code> regenerated. <code>svelteserver</code> is the exception, and is told the
  editor watches: left to its own devices it walks the entire workspace root, which in a repository
  that is a Svelte app <em>and</em> something much larger means tens of thousands of directories and
  a server that dies on <em>too many open files</em> before answering anything. The cost is that it
  learns about a change only when the file is opened or saved here.
</p>
<p>
  Because a server is selected by <strong>extension</strong>, enabling Angular puts
  <code>.html</code> into the language-server set for every project, including one that has no
  <code>angular.json</code> anywhere — where nothing starts, and go-to or rename on a plain page
  simply finds nothing. Turning Angular off in Settings → Language Servers takes
  <code>.html</code> back out.
</p>
<h2>A server you have not installed</h2>
<p>
  It does not appear in the running list. That list answers "what is serving this project", and a
  server whose binary was never there is not failing at that — it is the same sentence the table
  above already says, with an <strong>Install</strong> button next to it. Repeating it once per open
  project would fill the list with rows offering a Restart that can only fail again.
</p>
<p>
  Bennu remembers that it looked, so it does not search your <code>PATH</code> again on every
  keystroke, and forgets that the moment something could have changed the answer: installing a
  server, or saving the settings (which is how an executable path gets pinned by hand). Nothing to
  restart, nothing to clear.
</p>
<p>
  It also does not <em>claim</em> the language. A server that is starting up still owns its files
  — the correct answer while it warms up is "nothing yet", not an answer from some other engine —
  but a binary that is not installed is never going to answer at all, so the file falls through to
  whatever Bennu can do for it by itself. That is what keeps a <code>.wgsl</code> shader's go-to,
  find-usages, hover and compiler diagnostics working on a machine that has never installed
  wgsl-analyzer.
</p>
<h2>Where the binary is looked for</h2>
<p>
  In order: an explicit path you set, the language's own install location, <code>PATH</code>, then
  the places these tools generally install themselves. That last step matters more than it sounds:
  a windowed application does not inherit your shell's <code>PATH</code>, so
  <code>~/.cargo/bin</code>, <code>~/go/bin</code>, Homebrew's directory and npm's global prefix are
  all invisible to a plain <code>PATH</code> lookup even though your terminal finds them instantly.
  For rust-analyzer that also includes the <code>rustup</code> toolchains and the VS Code
  extension's copy.
</p>
<h3>Rust: the file that exists and does not work</h3>
<p>
  <code>~/.cargo/bin/rust-analyzer</code> is usually not the server. It is a <strong>rustup
  proxy</strong> — a link to <code>rustup</code> itself — and it is there whether or not the
  component is installed. Run it without the component and it exits immediately with
  <code>Unknown binary 'rust-analyzer' in official toolchain</code>.
</p>
<p>
  So Bennu looks in the <code>rustup</code> toolchains <em>before</em> <code>PATH</code> (the real
  binary should win from wherever it is), and when the only candidate left is a proxy for a
  component nobody installed, it reports <strong>not installed</strong> rather than showing a
  resolved path and then a server that dies. The fix is the one the panel prints:
</p>
<pre><code>rustup component add rust-analyzer</code></pre>
<p>
  A <code>cargo install</code>ed copy lives in the same directory and is a real binary — that one
  is used normally.
</p>
<h2>Starting, and what the footer says</h2>
<p>
  A server starts when a project is opened, not when you first ask it something — because a cold
  Rust workspace takes rust-analyzer tens of seconds to index, and a go-to that answers nothing
  during that time is indistinguishable from a go-to that does not work.
</p>
<p>
  The footer names the server for the open file and what it is doing: a spinner with its progress
  while it loads, its name once it is ready, a warning triangle when it is not running. Clicking
  it opens <strong>Settings → Language Servers</strong>.
</p>
<p>
  A server that fails to start <em>stays</em> failed until you restart it — deliberately, so that a
  server which is not installed is reported once instead of being respawned on every keystroke.
  <strong>Restart language server</strong> in the command palette (or the button in Settings) is
  the way back, and is also what to press after installing one.
</p>
<h2>The project root decides everything</h2>
<p>
  A server is started for a <strong>workspace root</strong> — the highest directory above the file
  that carries the language's project marker. In a Cargo workspace that is the top
  <code>Cargo.toml</code>, not each member's: one server over the whole graph is what makes
  cross-crate go-to work at all.
</p>
<p>
  It is also the gate that keeps things quiet. A stray <code>.py</code> in a Java repository has no
  <code>pyproject.toml</code> above it, so no Python server starts. Nothing runs unless there is a
  real project for it to analyse.
</p>

<h3>What it analyses: the whole workspace</h3>
<p>
  Not the open file. The server is handed the root, reads the project's own manifest, resolves the
  entire dependency graph, runs build scripts, expands procedural macros, and indexes every crate —
  which is what the progress line in the footer is doing, and why it takes tens of seconds on a cold
  workspace. Cross-crate <kbd>Ctrl</kbd>+<kbd>B</kbd> and <kbd>Ctrl</kbd>+<kbd>N</kbd> would be
  impossible otherwise.
</p>
<p>
  The only thing that is per-file is the <em>unsaved buffer</em>: the file you are editing is sent
  over as you type, and every other file the server reads from disk itself. And
  <strong>diagnostics are workspace-wide too</strong> — the <code>cargo check</code> that runs on save
  checks everything and reports on every file, so a problem can appear in a file you have never
  opened.
</p>
<p>
  One consequence worth knowing on a large workspace: that check and your own builds share
  <code>target/</code>, and cargo takes a lock on it. A <code>cargo build</code> started from the
  Cargo tool window right after a save can therefore sit waiting for the server's check to finish —
  "blocking waiting for file lock" is that, not a hang.
</p>

<h3>A dependency's source is not a workspace</h3>
<p>
  Following <kbd>Ctrl</kbd>+<kbd>B</kbd> into a library opens a file under
  <code>~/.cargo/registry/src</code>, and that unpacked crate has a <code>Cargo.toml</code> of its
  own — so by the rule above it would look like a workspace root and get a server of its own, one per
  library you looked into. It does not: those locations are known, and a file in one of them
  <strong>borrows</strong> the session that already has it. Your project's server has every
  dependency's source open already, because that is what resolving the crate graph means — and it is
  the only one that can answer about your code as well.
</p>
<p>
  With two projects of the same language open, a file inside a shared dependency gets no
  intelligence rather than an arbitrary one of the two: which server answered would otherwise depend
  on ordering, and a feature that works intermittently is worse than one that is honestly absent.
</p>
<h2>Renaming a file, and reloading the project</h2>
<p>
  <kbd>F2</kbd> in the Project tree renames a file <em>and</em> the code that referred to it by name:
  for Rust, the <code>mod</code> declaration naming it and every <code>use</code> path through the
  module it declares. The server is asked what the rename implies before anything moves, so the dialog
  can say how many files it will touch — and a rename that cannot be performed changes nothing at all.
  The edits are applied through the editor, so they are one undo step like any other change.
</p>
<p>
  <strong>Reload workspace</strong> in the command palette makes the server re-read the project's
  manifests and resolve the crate graph again, keeping everything it has already indexed — which a
  restart would throw away. rust-analyzer notices a <code>Cargo.toml</code> it knows about changing on
  its own; this is for what it cannot see: a <code>.cargo/config.toml</code> edit, a patched or
  vendored dependency changing underneath, a <code>cargo add</code> run in a terminal.
</p>
<h2>Adding a language</h2>
<p>
  Any server that speaks LSP over stdio can be added in <code>bennu/config.toml</code> — the same
  fields the built-in entries carry, so it gets the same features with no new release:
</p>
<pre><code>[lsp]
enabled = true
disabled = []          # server ids to turn off

[lsp.server_paths]
rust-analyzer = "/opt/ra/rust-analyzer"   # an explicit binary

[[lsp.servers]]
id = "zls"
name = "Zig"
language = "zig"                # the LSP languageId
command = "zls"
args = []                       # several servers need ["--stdio"]
extensions = ["zig", "zon"]     # no dots
root_markers = ["build.zig"]
initialization_options = ""     # server-specific JSON, as a string</code></pre>
<p>
  <code>extensions</code> and <code>root_markers</code> are both required: without an extension the
  entry can never be selected, and without a marker it can never start. An entry whose
  <code>id</code> matches a built-in <strong>replaces</strong> it — which is how a server is
  reconfigured (different arguments, different options), as opposed to merely re-pointed at another
  binary.
</p>
<h2>Limits worth knowing</h2>
<ul>
  <li><strong>Files are not created or deleted on a server's behalf.</strong> Bennu edits buffers
    through the editor so every change is undoable, and it performs a file <em>rename</em> itself
    (above). A refactoring that wants a new or deleted file says so instead of being half-applied.</li>
  <li><strong>Run and debug lenses are not offered.</strong> rust-analyzer can put a ▶ above every
    <code>fn main</code> and every test; Bennu does not ask for them, because a control that does
    nothing when pressed teaches that the feature is broken. Tests are run from the Cargo window.</li>
  <li><strong>Inlay hints</strong> (inferred types shown inline) are not rendered.</li>
  <li><strong>A macro expansion cannot be navigated</strong> — see above.</li>
</ul>
