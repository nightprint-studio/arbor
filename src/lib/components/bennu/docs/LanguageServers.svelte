<!-- Bennu docs — language servers: what they add, which are supported, and how to add one. -->
<h1>Language servers</h1>
<p class="doc-lead">
  Bennu's Java intelligence is its own engine. Every other language is served by an external
  <strong>language server</strong>, and the editor treats both the same way.
</p>

<h2>What you get</h2>
<p>
  For a file a server owns, the whole editing surface is live — the same gestures as in a Java
  file, answered by the server instead:
</p>
<ul>
  <li><strong>Completion</strong> as you type, and after the punctuation the language cares about
    (for Rust that is <code>.</code> and <code>::</code>). An item that needs an import brings the
    <code>use</code> line with it.</li>
  <li><strong>Go to declaration</strong> — <kbd>Ctrl</kbd> + <kbd>B</kbd> or
    <kbd>Ctrl</kbd>-click. On the declaration itself it flips to find-usages, as it does in Java.</li>
  <li><strong>Find usages</strong> — <kbd>Alt</kbd> + <kbd>F7</kbd>.</li>
  <li><strong>Hover</strong> — the signature, where the item lives, and its documentation.</li>
  <li><strong>Diagnostics</strong>, in the editor and in the Problems panel.</li>
  <li><strong>Rename</strong> — <kbd>Shift</kbd> + <kbd>F6</kbd>, with the same preview.</li>
  <li><strong>Quick fixes and refactorings</strong> under <kbd>Alt</kbd> + <kbd>Enter</kbd> — the
    server's, and on a file it owns they are the whole list: "import <code>HashMap</code>", "fill match
    arms", "inline macro". Fixes come first, then refactorings; an action the server offers but cannot
    apply here is shown last <em>with its reason</em> rather than hidden, because "selection crosses a
    block" tells you what to change and an absent row does not. Many of its refactorings want a
    <strong>selection</strong> — extract a variable or a function has to be told what — so at a bare
    caret the list is short by design.</li>
  <li><strong>Format</strong> — <kbd>Alt</kbd> + <kbd>Shift</kbd> + <kbd>F</kbd>, with the
    language's own formatter (<code>rustfmt</code> for Rust) and the project's own configuration.</li>
  <li><strong>Semantic colouring</strong> — see below.</li>
  <li><strong>Signature help</strong> — the parameter list of the call the caret is inside.</li>
  <li><strong>Go to type / symbol</strong> — <kbd>Ctrl</kbd> + <kbd>N</kbd> and
    <kbd>Ctrl</kbd> + <kbd>Shift</kbd> + <kbd>Y</kbd>, searched across the whole workspace. See
    below.</li>
  <li><strong>Structure</strong> — the outline in the Structure panel and in the
    <kbd>Ctrl</kbd> + <kbd>F12</kbd> popup, in the language's own vocabulary: a Rust file lists its
    structs, traits, impls and functions, with the same icons a Java type and member get.</li>
  <li><strong>Occurrence highlighting</strong> — every other place the symbol under the caret appears
    in this file, tinted as you rest on it. A write is tinted differently from a read, because "where
    is this assigned" is a different question from "where is this used".</li>
  <li><strong>Folding</strong> — from the server, so it folds by <em>item</em>: a <code>use</code>
    block, a doc comment, a <code>#[cfg]</code>-gated module, a match arm. The usual fold keys and the
    gutter arrows work on it.</li>
  <li><strong>Expand / shrink selection</strong> — <kbd>Alt</kbd> + <kbd>Shift</kbd> + <kbd>→</kbd>
    and <kbd>←</kbd>, one syntactic step at a time.</li>
  <li><strong>Placeholders</strong> in an accepted completion — <kbd>Tab</kbd> between them; see
    below.</li>
  <li><strong>Code lenses</strong> — counts above an item, clickable; see below.</li>
  <li><strong>Call and type hierarchy</strong> — <kbd>Ctrl</kbd> + <kbd>Shift</kbd> + <kbd>H</kbd> and
    <kbd>Ctrl</kbd> + <kbd>H</kbd>; see below.</li>
</ul>

<h2>Go to type, go to symbol</h2>
<p>
  On a project with no Java index, <kbd>Ctrl</kbd> + <kbd>N</kbd> asks the server instead. The tab
  is called <strong>Types</strong> rather than Classes, because what it finds are structs, enums,
  traits, unions and type aliases — and <kbd>Ctrl</kbd> + <kbd>Shift</kbd> + <kbd>Y</kbd>
  (<strong>Symbols</strong>) finds everything else: functions, methods, constants, statics, fields.
  The same split as the Java pair, in the vocabulary of the language actually open.
</p>
<p>
  The matching is done by the <em>server</em>, not here, which is why it needs at least two
  characters: a workspace is far too large to hand over whole, and a one-character query is a great
  many rows that discriminate nothing.
</p>
<p>
  Each row is <strong>named and marked in the language's own vocabulary</strong>. The protocol has a
  fixed list of 26 symbol kinds and every language squeezes into it — rust-analyzer reports a
  <code>trait</code> as an interface, an <code>impl</code> block as an object, a type alias as a type
  parameter — so those are translated back before they are shown. In <strong>Types</strong>, where
  every row is a type and a shape would say nothing, the mark is the lettered ring a Java class wears:
  <strong>S</strong> struct, <strong>T</strong> trait, <strong>E</strong> enum, <strong>A</strong> type
  alias. In <strong>Symbols</strong> it is a shape instead, because there the distinction worth
  drawing is function against constant against field.
</p>
<p>
  Two are deliberately <em>not</em> translated: a <code>union</code> arrives as a struct and a
  <code>static</code> as a constant, and nothing downstream can tell them apart — so both keep the
  protocol's word rather than being guessed at.
</p>

<h2>Code lenses</h2>
<p>
  A line of text above an item, saying how many things there are of a kind and taking you to them:
  <strong>implementations</strong> of a trait, and <strong>references</strong> to a type or a trait.
  Pressing one shows the list — one result jumps straight there, several open the same popover
  <kbd>Alt</kbd> + <kbd>F7</kbd> fills.
</p>
<p>
  It costs nothing extra to press: the server has already found those places, because finding them is
  how it counted them, and it sends the list along with the count. Reference counts are asked for on
  types and traits only — each one is a reference query per item, and methods are by far the most
  numerous items in a file.
</p>

<h2>Call and type hierarchy</h2>
<p>
  <kbd>Ctrl</kbd> + <kbd>Shift</kbd> + <kbd>H</kbd> on a function opens its <strong>callers</strong>;
  <kbd>Ctrl</kbd> + <kbd>H</kbd> on a type opens its <strong>implementors</strong>. Both land in the
  Hierarchy panel at the bottom, which has a direction chip to walk the other way — callees, or what a
  type is built on.
</p>
<p>
  The tree is expanded <strong>one level at a time</strong>, and that is not laziness for its own sake:
  expanding a call graph eagerly does not terminate. Mutual recursion, a trait implemented by a type
  that uses it, or simply a widely-called helper turns "expand everything" into a sweep of the
  workspace — so a recursive chain is something you walk into as far as you care to, and no further.
</p>
<p>
  A caller row jumps to the <em>call</em>, not to the head of the function containing it, and says
  <code>3×</code> when there is more than one call to the same thing inside it. The panel takes the
  keyboard as it opens: arrows walk and expand, <kbd>Enter</kbd> jumps.
</p>

<h2>Placeholders in a completion</h2>
<p>
  Accepting a completion that has holes in it puts the caret in the first one and
  <kbd>Tab</kbd> moves to the next — <code>println!</code> lands between the parentheses, a function
  lands on its first argument. <kbd>Shift</kbd> + <kbd>Tab</kbd> goes back, <kbd>Esc</kbd> leaves the
  run, and moving the caret anywhere else ends it.
</p>
<p>
  Two placeholders that the server marked as the <em>same</em> value are two stops you tab between
  rather than one that updates the other as you type. Nothing half-works: they behave as two ordinary
  stops.
</p>

<h2>Expanding a macro</h2>
<p>
  <kbd>Alt</kbd> + <kbd>Shift</kbd> + <kbd>M</kbd> on a macro call shows what it generates, in a
  dialog you can read, copy and re-expand from. It is also offered under
  <kbd>Alt</kbd> + <kbd>Enter</kbd>, named after the macro it found — <em>Expand vec!</em> — beside the
  server's own quick fixes, and only when the caret is genuinely inside a macro call. Worth knowing
  what it is and is not:
</p>
<ul>
  <li>the expansion is <strong>recursive</strong> — all the way down. The server offers no single-step
    form, so neither does Bennu;</li>
  <li>it is <strong>text</strong>, not a file the server knows, so there is no go-to, no hover and no
    completion inside it. To expand a macro <em>within</em> the expansion, point at it in the real
    source and expand again;</li>
  <li><strong>Re-expand</strong> asks about the caret as it is now — move it in the file behind the
    dialog and press it.</li>
</ul>

<h2>Semantic colouring</h2>
<p>
  A file is coloured twice. The first pass is local and instant: it recognises keywords, strings,
  comments and numbers, and it is what you see the moment a tab opens. The second comes from the
  server and refines it — a struct told apart from a trait, a macro from a function, a
  <code>mut</code> binding from an immutable one, a parameter from a local.
</p>
<p>
  The layers are deliberate. The local pass means a file is never a wall of plain text while a
  request is in flight, or when the server is not running at all; the semantic pass means that
  when it <em>is</em> running, the colours are facts rather than guesses.
</p>

<h2>Diagnostics arrive on save</h2>
<p>
  For Rust the real errors — types, borrows, unreachable code — come from
  <code>cargo check</code>, which the server runs when a file is <strong>saved</strong>. So they
  appear a moment after a save rather than as you type, and what you see while typing is what the
  parser alone can tell.
</p>
<p>
  <strong>Settings → Language Servers → Rust</strong> chooses what that command is:
  <code>cargo check</code>, or <code>cargo clippy</code> — a superset, every check error plus several
  hundred lints, at the cost of a slower build after every save. It is an option the server reads when
  it starts, so it takes effect on the next start.
</p>
<p>
  Autosave counts as a save, so with it on (the default) the loop is: stop typing, wait a beat,
  see the compiler's answer.
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

<h2>Supported out of the box</h2>
<table>
  <thead><tr><th>Language</th><th>Server</th><th>Files</th><th>Project marker</th></tr></thead>
  <tbody>
    <tr><td>Rust</td><td><code>rust-analyzer</code></td><td><code>.rs</code></td><td><code>Cargo.toml</code></td></tr>
    <tr><td>Go</td><td><code>gopls</code></td><td><code>.go</code></td><td><code>go.mod</code></td></tr>
    <tr><td>Python</td><td>Pyright</td><td><code>.py</code></td><td><code>pyproject.toml</code>, <code>setup.py</code></td></tr>
    <tr><td>C / C++</td><td><code>clangd</code></td><td><code>.c</code>, <code>.cpp</code>, <code>.h</code>, …</td><td><code>compile_commands.json</code>, <code>CMakeLists.txt</code></td></tr>
    <tr><td>TypeScript / JavaScript</td><td><code>typescript-language-server</code></td><td><code>.ts</code>, <code>.tsx</code>, <code>.js</code>, …</td><td><code>tsconfig.json</code>, <code>package.json</code></td></tr>
    <tr><td>Lua</td><td><code>lua-language-server</code></td><td><code>.lua</code></td><td><code>.luarc.json</code>, <code>plugin.toml</code></td></tr>
  </tbody>
</table>
<p>
  Bennu does not install any of them. Settings → Language Servers lists each one with the path it
  resolved to, or — when it found nothing — how to install it.
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
