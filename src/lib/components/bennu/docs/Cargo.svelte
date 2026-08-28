<!-- Bennu docs — the Rust side: the Cargo tool window, manifest intelligence, and cargo runs. -->
<h1>Rust &amp; Cargo</h1>
<p class="doc-lead">
  What a Cargo workspace is, what you can run on it, and what is wrong with its manifests — all of
  it read from the files rather than asked of <code>cargo</code>.
</p>

<h2>The Cargo tool window</h2>
<p>
  <kbd>Alt</kbd> + <kbd>8</kbd> opens it, in the same rail slot Maven takes on a Java project: a
  project is one or the other, and the key means "the build tool" either way.
</p>
<p>
  The first group is the <strong>whole workspace</strong> — every command, aimed at every crate
  (<code>--workspace</code>). Under it, one group per crate, each holding three things:
</p>
<ul>
  <li><strong>Commands</strong> — <code>check</code>, <code>build</code>, <code>test</code>,
    <code>run</code>, <code>clippy</code>, <code>fmt</code>, <code>doc</code>, each aimed at that
    crate with <code>-p</code>. Clicking one runs it; the output lands in the Run console.</li>
  <li><strong>Targets</strong> — the binaries, examples, integration tests and benchmarks, plus the
    library. Most of these are not in the manifest at all: <code>src/main.rs</code> and
    <code>src/bin/*.rs</code> are Cargo's own conventions, so they are discovered rather than read,
    and each row says which it was. A binary or an example <em>runs</em> when clicked; anything else
    opens its source.</li>
  <li><strong>Features</strong> — what the crate declares, with the ones <code>default</code>
    reaches (transitively) marked, and what each one turns on. An optional dependency's implicit
    feature is in the list too — unless something already refers to it as <code>dep:…</code>, which
    is Cargo's own rule for suppressing it.</li>
</ul>

<p>
  The crate header answers one more question, the one the three groups cannot: <strong>where the
  crate is</strong>. Its <strong>locate</strong> button — or <strong>Focus in Project</strong> from a
  right-click on the header — opens the Project tree on that crate's folder, expanded, selected and
  holding the keyboard focus, so the arrows carry on from there. The same menu opens the crate's
  <code>Cargo.toml</code> and copies its path.
</p>

<p>
  The common commands are in the <strong>command palette</strong> too
  (<kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>P</kbd>, "Cargo: …"), aimed at the whole workspace — the
  panel has to be open to click a row in it, and reaching a verb from wherever you are is what the
  palette is for.
</p>

<h3>Two things it will tell you unprompted</h3>
<p>
  A <strong>crate the workspace forgot</strong>: a directory under the root with a
  <code>Cargo.toml</code> that no <code>members</code> pattern covers. The failure is silent
  otherwise — the crate compiles when you build it directly and is invisible to
  <code>--workspace</code> — so the panel names it and offers to open the manifest that is missing
  it. A crate inside another crate's tree is not flagged: that is how a fixture crate is written.
</p>
<p>
  A <strong>missing toolchain component</strong>: <code>cargo clippy</code> without the component
  installed fails with an unknown-subcommand error, which reads as a broken button rather than a
  missing install. The row says <em>needs clippy</em> instead, and
  <code>rustup component add clippy</code> followed by ⟳ fixes it. When <code>rustup</code> is not
  there at all nothing is greyed out — not knowing is not the same as knowing it is absent.
</p>

<h2><code>Cargo.toml</code></h2>
<p>
  A manifest gets completion and diagnostics of its own. Both come from one description of what a
  manifest may contain, which is why a key that completes can never be a key that then underlines
  itself as unknown.
</p>

<h3>Completion</h3>
<p>
  <kbd>Ctrl</kbd> + <kbd>Shift</kbd> + <kbd>Space</kbd>, or just type. What is offered depends on where the caret is:
</p>
<ul>
  <li><strong>in a <code>[header]</code></strong> — the tables, with the ones the file already has
    left out (except <code>[[bin]]</code> and friends, where a second one is the point);</li>
  <li><strong>on a key</strong> — the keys of that table, minus the ones already set, each with a
    line saying what it does;</li>
  <li><strong>after a dot</strong> — <code>workspace</code>, on the keys Cargo actually lets a
    member inherit. On <code>name</code>, which it does not, nothing is offered;</li>
  <li><strong>in a dependency table</strong> — crate <em>names</em>, from this workspace, from
    <code>Cargo.lock</code> and from the crates already downloaded on this machine. Accepting one
    writes the whole assignment, newest known version included;</li>
  <li><strong>on a version</strong> — the versions this machine has, newest first (and a
    pre-release never sorts above the release it precedes);</li>
  <li><strong>inside a spec</strong> — <code>version</code>, <code>path</code>, <code>git</code>,
    <code>features</code>, <code>optional</code>, <code>workspace</code>, …;</li>
  <li><strong>in a value with a closed set</strong> — the editions, the crate types, the lint
    levels; quotes are supplied when the caret is not already inside a string;</li>
  <li><strong>in <code>[features]</code></strong> — the other features, the optional dependencies,
    and both reference forms (<code>dep:serde</code>, <code>serde/</code>);</li>
  <li><strong>in <code>members</code></strong> — the directories that actually hold a crate, plus
    the <code>dir/*</code> glob that covers what is under one.</li>
</ul>
<p>
  Completion itself never waits on the network: the crate list is what this machine has seen — this
  workspace, <code>Cargo.lock</code>, and the crates already downloaded — so the popup appears while
  you are still typing. What crates.io <em>does</em> answer is the two questions below, both off the
  same cached index.
</p>

<h3>Newer versions</h3>
<p>
  A dependency that is behind gets a line above it saying which release exists —
  <em>1.0.219 available</em> — and pressing it writes that version into the manifest, as one undo step.
</p>
<p>
  It is deliberately quiet. Nothing is said about a <code>path</code>, a <code>git</code> or a
  workspace-inherited dependency, because there is no version there to be behind; nothing about a
  deliberate pin (<code>=1.2.3</code>) or a range, because you already decided; and nothing about a
  pre-release, in either direction. A wrong "update available" on a pin is worse than a missing one.
</p>
<p>
  The answers come from an on-disk cache, refreshed at most once a day per crate, so in the steady
  state this is a file read. A failed lookup falls back to whatever was cached however old — offline,
  last week's version list is a better answer than silence.
  <strong>Settings → Language Servers → Rust</strong> turns the whole thing off, which makes Bennu
  entirely local again.
</p>

<h3>Adding a dependency</h3>
<p>
  The <strong>＋</strong> in the Cargo window's toolbar (or <em>Add dependency…</em> in the palette)
  takes a crate name, offers its published versions and the features <em>that version</em> declares,
  and lets you pick the table (<code>dependencies</code>, <code>dev-</code>, <code>build-</code>) and,
  in a workspace, which member to add it to.
</p>
<p>
  It runs the real <code>cargo add</code>. That matters: cargo writes the requirement the way it would
  write it, honours <code>[workspace.dependencies]</code> inheritance, validates the features against
  the crate it just resolved, and formats the entry in the file's own style. When it refuses, its own
  words are shown — they are the fix. Afterwards the manifest is re-read, the crate graph refreshed and
  the language server told to resolve the project again.
</p>
<p>
  There is no search box, because the index Cargo itself uses has no search: you look a crate up by
  name. If the name is unknown or the index is unreachable, the crate can still be added — leave the
  version empty and cargo picks it.
</p>

<h3>Diagnostics</h3>
<p>
  The two worth the feature on their own:
</p>
<ul>
  <li>a <strong>key typo</strong> — <code>[dependancies]</code>, <code>feature = […]</code>. Cargo
    ignores these silently, so the symptom is a dependency that is not there or a feature that does
    nothing, and the manifest looks fine;</li>
  <li>a <strong>feature referring to something that does not exist</strong>. Cargo refuses the
    manifest over it, and the usual cause is a rename. All four reference forms are understood, and
    a bare name that is a non-optional dependency gets its own wording, because the fix is
    <code>optional = true</code> and not inventing a feature.</li>
</ul>
<p>
  Plus: <code>workspace = true</code> with no matching entry in the root's
  <code>[workspace.dependencies]</code> or <code>[workspace.package]</code>; a <code>path</code> or
  a <code>members</code> entry with no crate behind it; a duplicate key; a dependency naming both a
  <code>git</code> and a <code>path</code>, or naming no source at all; an optional
  dev-dependency; a version requirement with no number in it; a <code>default-members</code> entry
  that is not a member.
</p>
<p>
  <strong>Severity means something here.</strong> Red is reserved for what Cargo genuinely refuses
  to build. Every unknown key is a warning, because Cargo warns rather than failing on those and
  gains new ones every few releases. And nothing is reported about a table Bennu does not
  recognise, or about anything under <code>[package.metadata]</code> and <code>[lints.*]</code> —
  those belong to other tools.
</p>

<h2>Dependencies</h2>
<p>
  <kbd>Alt</kbd> + <kbd>N</kbd> opens the same panel a Maven project uses, answering the same four
  questions with Cargo's vocabulary: one group per crate, and per row the crate name, the version,
  <strong>where that version came from</strong>, its kind (<code>normal</code> / <code>dev</code> /
  <code>build</code>), and whether it is actually in the local registry.
</p>
<p>
  The version shown is the one <code>Cargo.lock</code> chose, not the requirement you wrote —
  <code>serde = "1"</code> is not what you are compiling against. With no lockfile the panel says
  so and shows the requirement instead, which is <em>unknown</em> rather than <em>missing</em>.
  A <code>workspace = true</code> dependency is marked as pinned by the workspace, the same way a
  Maven <code>&lt;dependencyManagement&gt;</code> entry is; a renamed one shows both names; a
  target-specific one carries its <code>cfg(…)</code>, because whether it is on the graph depends on
  what you are building for.
</p>
<p>
  The last group is everything in the lockfile that no crate of yours declares — what your
  dependencies dragged in.
</p>

<h2>Running</h2>
<p>
  ▶ and <kbd>Shift</kbd> + <kbd>F10</kbd> launch the active run configuration, and a
  <strong>Cargo</strong> configuration is a cargo subcommand: a crate, a command, a target, features,
  a profile, and two separate argument fields. On a workspace with exactly one binary you need none
  of it — press ▶ and Bennu makes the configuration and runs it. With several, it opens the editor,
  because <code>cargo run</code> refuses to guess too.
</p>
<p>
  The two argument fields are not interchangeable: <strong>Cargo arguments</strong> go before the
  <code>--</code> and reach cargo, <strong>Program arguments</strong> go after it and reach your
  program or the test harness. <code>--nocapture</code> belongs in the second.
</p>
<p>
  A cargo run has no build step in front of it — the command <em>is</em> the build. Its output,
  Stop, ⟳ and the tab strip all behave exactly as they do for a JVM launch, because it is the same
  console and the same process registry. What is <strong>not</strong> there is 🐞: debugging attaches
  JDWP to a JVM Bennu started, and a cargo command forks its own compiler and its own program.
</p>
<h2>Navigating</h2>
<p>
  <kbd>Ctrl</kbd> + <kbd>N</kbd> opens the navigator with its first tab reading <strong>Types</strong>
  rather than Classes — what it finds are structs, enums, traits and type aliases, and calling them
  classes would describe a language this project is not written in.
  <kbd>Ctrl</kbd> + <kbd>Shift</kbd> + <kbd>Y</kbd> finds the functions, methods and constants. The
  language server does the matching, so both start at two characters: a workspace is too large to
  hand over whole.
</p>
<p>
  The <strong>Trees</strong> panel is not offered here. Both of its views read Bennu's own engines —
  the tree-sitter grammars are Java and JSP, the declaration model is Java's — so on a Rust project
  it could only ever report their absence, and "no grammar for Rust" reads as though Bennu did not
  understand the language when in fact rust-analyzer is answering everything else about it. The rail
  slot holds the Cargo window instead.
</p>
<p>
  See <em>Building &amp; running</em> for the run configurations themselves, and
  <em>Language servers</em> for what rust-analyzer adds to a <code>.rs</code> file.
</p>
