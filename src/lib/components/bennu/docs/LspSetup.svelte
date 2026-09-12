<script lang="ts">
  /**
   * Installing a language server: the servers Bennu knows, installing one from the app, where the binary is looked for,
   * starting and the footer, the project root, file renames and reloads, adding a language, and the limits.
   */
  import Callout from '$lib/components/shared/ui/Callout.svelte';
  import { highlightCode } from '$lib/utils/highlight';
</script>

<span class="eyebrow">Reference</span>
<h1>Installing a language server</h1>

<p class="doc-lead">
  Which servers Bennu knows about, how to install one from inside the app, where it looks for the binary, and why the project root decides what a server can see.
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

<h2>Installing one</h2>
<p>
  <strong>Settings → Language Servers</strong> lists each one with the path it resolved to, or — when it found nothing — how to install it. For the ones distributed through a
  package manager you already have (<code>rustup</code>, <code>cargo</code>, <code>go</code>, <code>npm</code>) an <strong>Install</strong> button beside the hint runs exactly that
  command, streaming into the Build panel: <code>cargo install --git</code> builds a server from source and takes minutes, so it is worth watching rather than waiting on.
</p>
<div class="feature-grid two-col">
  <div class="feature-card">
    <div class="fc-eyebrow">Not a download</div>
    <div class="fc-title">A command, on purpose</div>
    <div class="fc-desc">The server lands where the rest of your toolchain lives, so it keeps working after Arbor is updated or removed, and <code>wgsl-analyzer --version</code> says the same thing in your own terminal.</div>
  </div>
  <div class="feature-card">
    <div class="fc-eyebrow">No button</div>
    <div class="fc-title">System packages</div>
    <div class="fc-desc"><code>clangd</code> is LLVM, <code>lua-language-server</code> is Homebrew. Bennu installs language servers, not toolchains.</div>
  </div>
</div>
<Callout variant="info" title="WGSL works without one">
  A shader gets diagnostics, completion and find usages whether or not <code>wgsl-analyzer</code> is installed — see <strong>Shaders (WGSL)</strong>, under Rust. The server, when present,
  serves the file instead, which is worth knowing before installing it on a <strong>Bevy</strong> project: its module system is WESL rather than naga_oil, so it does not understand
  <code>#import</code>, and it replaces Bennu's deliberate silence on a composed shader with its own reading of those lines.
</Callout>

<h2>Svelte and Angular</h2>
<p>
  A <code>.svelte</code> file is coloured as markup with <code>&lt;script&gt;</code> and <code>&lt;style&gt;</code> in it, immediately and with no server. What that cannot know — that a name
  is a component, a prop, a store — arrives from <code>svelteserver</code> as semantic tokens layered on top, along with completion, hover, diagnostics, go-to, find usages and rename. The
  template syntax itself (<code>&#123;#each&#125;</code>, <code>&#123;expr&#125;</code>) stays uncoloured: no HTML grammar can be told it is Svelte, and pretending otherwise would colour it
  wrongly rather than not at all.
</p>

<h3>Angular serves templates, not <code>.ts</code></h3>
<p>
  Its server also speaks TypeScript, but one server serves a file here and the first match wins for <em>every</em> project — so claiming <code>.ts</code> would take every TypeScript file on
  the machine away from <code>typescript-language-server</code>, Angular project or not. Templates are the half that otherwise has nothing, so this is the trade that only adds. A
  <code>.ts</code> file keeps the server it has always had, which means a rename started in a component's class does not follow the name into its template.
</p>
<p>
  Wanting the other trade is a <code>[[lsp.servers]]</code> entry with <code>id = "angular"</code>: a custom entry shadows the built-in completely, so its <code>extensions</code> and its
  <code>args</code> are the ones used. Spell out the probe locations if you do — <code>ngserver</code> is a front end for the TypeScript and Angular language services and locates them from
  the command line; without them it starts, handshakes, and answers nothing.
</p>
<Callout variant="warning" title="Enabling Angular claims .html everywhere">
  A server is selected by <strong>extension</strong>, so <code>.html</code> joins the language-server set for every project, including one with no <code>angular.json</code> anywhere — where
  nothing starts, and go-to or rename on a plain page simply finds nothing. Turning Angular off in <strong>Settings → Language Servers</strong> takes <code>.html</code> back out.
</Callout>

<h3>Who watches the files</h3>
<p>
  A language server either watches the project for outside changes itself, or trusts the editor to tell it. Most watch for themselves, which is what Bennu wants — it does not deliver those
  notifications, so a server that stopped watching would quietly stop noticing a file <code>cargo build</code> regenerated.
</p>
<Callout variant="info" title="svelteserver is the exception">
  It is told the editor watches: left to itself it walks the entire workspace root, which in a repository that is a Svelte app <em>and</em> something much larger means tens of thousands of
  directories and a server that dies on <em>too many open files</em> before answering anything. The cost is that it learns about a change only when the file is opened or saved here — which looks
  like a Bennu bug when it bites.
</Callout>

<h2>A server you have not installed</h2>
<dl class="meta-grid">
  <dt>Not in the running list</dt>
  <dd>That list answers "what is serving this project", and a server whose binary was never there is not failing at that — the table above already says so, with an <strong>Install</strong> button. Repeating it per open project would fill the list with rows offering a Restart that can only fail again.</dd>
  <dt>Looked for once</dt>
  <dd>Bennu remembers that it looked, so it does not search your <code>PATH</code> on every keystroke, and forgets the moment something could change the answer: installing a server, or saving the settings — which is how an executable path gets pinned by hand. Nothing to restart, nothing to clear.</dd>
  <dt>It does not claim the language</dt>
  <dd>A server starting up still owns its files — the correct answer while it warms up is "nothing yet", not an answer from another engine. A binary that is not installed is never going to answer, so the file falls through to whatever Bennu can do by itself. That is what keeps a <code>.wgsl</code> shader's go-to, find usages, hover and compiler diagnostics working on a machine that never installed wgsl-analyzer.</dd>
</dl>

<h2>Where the binary is looked for</h2>
<ol class="step-list">
  <li>An explicit path you set.</li>
  <li>The language's own install location.</li>
  <li><code>PATH</code>.</li>
  <li>The places these tools generally install themselves.</li>
</ol>
<p>
  The last step matters more than it sounds: a windowed application does not inherit your shell's <code>PATH</code>, so <code>~/.cargo/bin</code>, <code>~/go/bin</code>, Homebrew's directory
  and npm's global prefix are all invisible to a plain <code>PATH</code> lookup even though your terminal finds them instantly. For rust-analyzer it also includes the <code>rustup</code>
  toolchains and the VS Code extension's copy.
</p>

<h3>Rust: the file that exists and does not work</h3>
<p>
  <code>~/.cargo/bin/rust-analyzer</code> is usually not the server. It is a <strong>rustup proxy</strong> — a link to <code>rustup</code> itself — there whether or not the component is
  installed. Run without the component, it exits immediately with <code>Unknown binary 'rust-analyzer' in official toolchain</code>.
</p>
<p>
  So Bennu looks in the <code>rustup</code> toolchains <em>before</em> <code>PATH</code> — the real binary should win from wherever it is — and when the only candidate left is a proxy for a
  component nobody installed, it reports <strong>not installed</strong> rather than a resolved path and then a server that dies. The fix is the one the panel prints:
</p>
<pre><code>{@html highlightCode(`rustup component add rust-analyzer`, 'bash')}</code></pre>
<p>A <code>cargo install</code>ed copy lives in the same directory and is a real binary — that one is used normally.</p>

<h2>Starting, and what the footer says</h2>
<p>
  A server starts when a project is opened, not when you first ask it something: a cold Rust workspace takes rust-analyzer tens of seconds to index, and a go-to that answers nothing during that
  time is indistinguishable from a go-to that does not work.
</p>
<table>
  <thead><tr><th>Footer</th><th>Means</th></tr></thead>
  <tbody>
    <tr><td>A spinner with progress</td><td>The server for the open file is loading</td></tr>
    <tr><td>Its name</td><td>Ready</td></tr>
    <tr><td>A warning triangle</td><td>Not running</td></tr>
  </tbody>
</table>
<p>Clicking it opens <strong>Settings → Language Servers</strong>.</p>
<Callout variant="tip" title="A failed server stays failed until you restart it">
  Deliberately, so a server that is not installed is reported once instead of respawned on every keystroke. <strong>Restart language server</strong> in the command palette, or the button in
  Settings, is the way back — and what to press after installing one.
</Callout>

<h2>The project root decides everything</h2>
<p>
  A server is started for a <strong>workspace root</strong> — the highest directory above the file carrying the language's project marker. In a Cargo workspace that is the top
  <code>Cargo.toml</code>, not each member's: one server over the whole graph is what makes cross-crate go-to work at all.
</p>
<p>
  It is also the gate that keeps things quiet. A stray <code>.py</code> in a Java repository has no <code>pyproject.toml</code> above it, so no Python server starts. Nothing runs unless there is
  a real project for it to analyse.
</p>

<h3>What it analyses: the whole workspace</h3>
<p>
  Not the open file. The server is handed the root, reads the project's manifest, resolves the entire dependency graph, runs build scripts, expands procedural macros and indexes every crate —
  what the footer's progress line is doing, and why it takes tens of seconds on a cold workspace. Cross-crate <kbd>Ctrl</kbd> + <kbd>B</kbd> and <kbd>Ctrl</kbd> + <kbd>N</kbd> would be
  impossible otherwise.
</p>
<ul>
  <li>The only per-file thing is the <em>unsaved buffer</em>: the file you are editing is sent as you type, every other file the server reads from disk.</li>
  <li><strong>Diagnostics are workspace-wide too</strong> — the <code>cargo check</code> run on save checks everything, so a problem can appear in a file you never opened.</li>
</ul>
<Callout variant="info" title="&quot;Blocking waiting for file lock&quot; is not a hang">
  That check and your own builds share <code>target/</code>, and cargo locks it. On a large workspace a <code>cargo build</code> started from the Cargo tool window right after a save can sit
  waiting for the server's check to finish.
</Callout>

<h3>A dependency's source is not a workspace</h3>
<p>
  Following <kbd>Ctrl</kbd> + <kbd>B</kbd> into a library opens a file under <code>~/.cargo/registry/src</code>, and that unpacked crate has a <code>Cargo.toml</code> of its own — so by the rule
  above it would look like a root and get a server of its own, one per library you looked into. It does not: those locations are known, and a file in one <strong>borrows</strong> the session
  that already has it. Your project's server has every dependency's source open already, because that is what resolving the crate graph means — and it is the only one that can answer about
  your code as well.
</p>
<p>
  With two projects of the same language open, a file inside a shared dependency gets no intelligence rather than an arbitrary one of the two: which server answered would depend on ordering,
  and a feature that works intermittently is worse than one that is honestly absent.
</p>

<h2>Renaming a file, and reloading the project</h2>
<dl class="meta-grid">
  <dt><kbd>F2</kbd> in the Project tree</dt>
  <dd>Renames a file <em>and</em> the code referring to it by name: for Rust, the <code>mod</code> declaration naming it and every <code>use</code> path through the module it declares. The server is asked what the rename implies before anything moves, so the dialog says how many files it will touch — and a rename that cannot be performed changes nothing. The edits go through the editor, one undo step like any other change.</dd>
  <dt>Reload workspace</dt>
  <dd>In the command palette: the server re-reads the manifests and resolves the crate graph again, keeping what it has indexed — which a restart would throw away. rust-analyzer notices a <code>Cargo.toml</code> it knows about changing on its own; this is for what it cannot see — a <code>.cargo/config.toml</code> edit, a patched or vendored dependency changing underneath, a <code>cargo add</code> run in a terminal.</dd>
</dl>

<h2>Adding a language</h2>
<p>
  Any server speaking LSP over stdio can be added in <code>bennu/config.toml</code> — the same fields the built-in entries carry, so it gets the same features with no new release:
</p>
<pre><code>{@html highlightCode(`[lsp]
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
initialization_options = ""     # server-specific JSON, as a string`, 'toml')}</code></pre>
<ul>
  <li><code>extensions</code> and <code>root_markers</code> are both required: without an extension the entry can never be selected, without a marker it can never start.</li>
  <li>An entry whose <code>id</code> matches a built-in <strong>replaces</strong> it — how a server is reconfigured (other arguments, other options), as opposed to re-pointed at another binary.</li>
</ul>

<h2>Limits worth knowing</h2>
<dl class="meta-grid">
  <dt>No files created or deleted for a server</dt>
  <dd>Bennu edits buffers through the editor so every change is undoable, and performs a file <em>rename</em> itself (above). A refactoring wanting a new or deleted file says so instead of being half-applied.</dd>
  <dt>Only lenses that do something</dt>
  <dd>rust-analyzer's ▶ Run and Debug lenses above <code>fn main</code> and every test are asked for because the Run console launches exactly what they carry — see <strong>Building &amp; running</strong>. A lens whose press would do nothing is not requested: a control that does nothing teaches that the feature is broken.</dd>
  <dt>Inlay hints</dt>
  <dd>Inferred types shown inline are not rendered for a language-server file.</dd>
  <dt>Macro expansions</dt>
  <dd>Cannot be navigated — see <strong>Language servers</strong>.</dd>
</dl>
