<script lang="ts">
  /**
   * Rust & Cargo: the Cargo tool window, what a manifest gets — completion, newer versions, adding a dependency,
   * diagnostics — the Dependencies panel, running, and navigating a Rust project.
   */
  import Callout from '$lib/components/shared/ui/Callout.svelte';
  import { highlightCode } from '$lib/utils/highlight';
</script>

<span class="eyebrow">Rust</span>
<h1>Rust &amp; Cargo</h1>

<p class="doc-lead">
  What a Cargo workspace is, what you can run on it, and what is wrong with its manifests — all of it read from the files rather than asked of
  <code>cargo</code>.
</p>

<h2>The Cargo tool window</h2>
<p>
  <kbd>Alt</kbd> + <kbd>8</kbd> opens it, in the rail slot Maven takes on a Java project: a project is one or the other, and the key means "the build tool"
  either way.
</p>
<p>
  The first group is the <strong>whole workspace</strong> — every command aimed at every crate, <code>--workspace</code>. Under it, one group per crate,
  each holding three things:
</p>
<div class="feature-grid">
  <div class="feature-card">
    <div class="fc-eyebrow">Click to run, with <code>-p</code></div>
    <div class="fc-title">Commands</div>
    <div class="fc-desc"><code>check</code>, <code>build</code>, <code>test</code>, <code>run</code>, <code>clippy</code>, <code>fmt</code>, <code>doc</code> — output in the Run console.</div>
  </div>
  <div class="feature-card">
    <div class="fc-eyebrow">Discovered, not only read</div>
    <div class="fc-title">Targets</div>
    <div class="fc-desc">The binaries, examples, integration tests, benchmarks and the library. <code>src/main.rs</code> and <code>src/bin/*.rs</code> are Cargo's conventions, not the manifest's, and each row says which it was. A binary or example <em>runs</em> when clicked; anything else opens its source.</div>
  </div>
  <div class="feature-card">
    <div class="fc-eyebrow">What the crate declares</div>
    <div class="fc-title">Features</div>
    <div class="fc-desc">The ones <code>default</code> reaches, transitively, are marked, with what each turns on. An optional dependency's implicit feature is listed — unless something refers to it as <code>dep:…</code>, Cargo's own rule for suppressing it.</div>
  </div>
</div>
<p>
  The crate header answers what the three groups cannot — <strong>where the crate is</strong>. Its <strong>locate</strong> button, or <strong>Focus in
  Project</strong> from a right-click on the header, opens the Project tree on the crate's folder, expanded, selected and holding the keyboard focus. The
  same menu opens its <code>Cargo.toml</code> and copies its path.
</p>
<p>
  The common commands are in the <strong>command palette</strong> too — "Cargo: …", aimed at the whole workspace — since a row can only be clicked with the
  panel open.
</p>

<h3>Two things it tells you unprompted</h3>
<Callout variant="warning" title="A crate the workspace forgot">
  A directory under the root with a <code>Cargo.toml</code> no <code>members</code> pattern covers. Otherwise silent: it compiles when built directly and is
  invisible to <code>--workspace</code>. The panel names it and offers to open the manifest missing it. A crate inside another crate's tree is not flagged —
  that is how a fixture crate is written.
</Callout>
<Callout variant="info" title="A missing toolchain component">
  <code>cargo clippy</code> without the component fails with an unknown-subcommand error, which reads as a broken button. The row says <em>needs clippy</em>
  instead, and <code>rustup component add clippy</code> then ⟳ fixes it. Without <code>rustup</code> at all nothing is greyed: not knowing is not knowing it
  is absent.
</Callout>

<h2><code>Cargo.toml</code></h2>
<p>
  A manifest gets completion and diagnostics of its own, both from one description of what a manifest may contain — which is why a key that completes can
  never underline itself as unknown.
</p>

<h3>Completion</h3>
<p><kbd>Ctrl</kbd> + <kbd>Shift</kbd> + <kbd>Space</kbd>, or just type. What is offered depends on the caret:</p>
<table>
  <thead><tr><th>The caret</th><th>Offered</th></tr></thead>
  <tbody>
    <tr><td>In a <code>[header]</code></td><td>The tables, minus those already there — except <code>[[bin]]</code> and friends, where a second is the point</td></tr>
    <tr><td>On a key</td><td>The table's keys not yet set, each with a line saying what it does</td></tr>
    <tr><td>After a dot</td><td><code>workspace</code>, on the keys Cargo lets a member inherit — nothing on <code>name</code>, which it does not</td></tr>
    <tr><td>In a dependency table</td><td>Crate <em>names</em> from this workspace, <code>Cargo.lock</code> and the crates already downloaded; accepting writes the whole assignment with the newest known version</td></tr>
    <tr><td>On a version</td><td>The versions this machine has, newest first — a pre-release never above its release</td></tr>
    <tr><td>Inside a spec</td><td><code>version</code>, <code>path</code>, <code>git</code>, <code>features</code>, <code>optional</code>, <code>workspace</code>, …</td></tr>
    <tr><td>A value with a closed set</td><td>Editions, crate types, lint levels — with quotes when the caret is not in a string</td></tr>
    <tr><td>In <code>[features]</code></td><td>The other features, the optional dependencies, and both reference forms, <code>dep:serde</code> and <code>serde/</code></td></tr>
    <tr><td>In <code>members</code></td><td>The directories holding a crate, and the <code>dir/*</code> glob covering what is under one</td></tr>
  </tbody>
</table>
<Callout variant="tip" title="Completion never waits on the network">
  The crate list is what this machine has seen, so the popup appears while you type. What crates.io answers is the two questions below, off the same cached index.
</Callout>

<h3>Newer versions</h3>
<p>
  A dependency that is behind gets a line above it — <em>1.0.219 available</em> — and pressing it writes that version into the manifest as one undo step. It is
  deliberately quiet:
</p>
<ul>
  <li>nothing about a <code>path</code>, <code>git</code> or workspace-inherited dependency, which has no version to be behind;</li>
  <li>nothing about a pin (<code>=1.2.3</code>) or a range — you already decided;</li>
  <li>nothing about a pre-release, either way.</li>
</ul>
<p>
  Answers come from an on-disk cache refreshed at most daily per crate, and a failed lookup falls back to whatever was cached — offline, last week's list beats
  silence. <strong>Settings → Rust</strong> turns it off, making Bennu entirely local again.
</p>

<h3>Adding a dependency</h3>
<ol class="step-list">
  <li>Press the <strong>＋</strong> in the Cargo window's toolbar, or <em>Add dependency…</em> in the palette.</li>
  <li>Type the crate name, then pick a published version and the features <em>that version</em> declares.</li>
  <li>Pick the table — <code>dependencies</code>, <code>dev-</code>, <code>build-</code> — and, in a workspace, the member.</li>
</ol>
<p>
  It runs the real <code>cargo add</code>: the requirement written cargo's way, <code>[workspace.dependencies]</code> inheritance honoured, the features validated
  against the resolved crate, the entry in the file's own style. When cargo refuses, its words are shown — they are the fix. Afterwards the manifest is read again,
  the crate graph refreshed and the language server told to resolve the project.
</p>
<Callout variant="info" title="There is no search box">
  The index Cargo uses has no search: a crate is looked up by name. An unknown name or an unreachable index still adds — leave the version empty and cargo picks it.
</Callout>

<h3>Diagnostics</h3>
<pre><code>{@html highlightCode(`[dependancies]              # a typo: Cargo ignores the table, and the dependency is not there
serde = "1"

[features]
json = ["dep:serde_json"]    # nothing called serde_json: Cargo refuses the manifest`, 'toml')}</code></pre>
<div class="feature-grid two-col">
  <div class="feature-card">
    <div class="fc-eyebrow">Silent in Cargo</div>
    <div class="fc-title">A key typo</div>
    <div class="fc-desc"><code>[dependancies]</code>, <code>feature = […]</code> — the symptom is a dependency that is not there or a feature that does nothing, in a manifest that looks fine.</div>
  </div>
  <div class="feature-card">
    <div class="fc-eyebrow">Refused by Cargo</div>
    <div class="fc-title">A feature referring to nothing</div>
    <div class="fc-desc">Usually after a rename. All four reference forms are understood, and a bare name that is a non-optional dependency gets its own wording — the fix is <code>optional = true</code>.</div>
  </div>
</div>
<p>Plus:</p>
<ul>
  <li><code>workspace = true</code> with no matching entry in the root's <code>[workspace.dependencies]</code> or <code>[workspace.package]</code>;</li>
  <li>a <code>path</code> or <code>members</code> entry with no crate behind it; a <code>default-members</code> entry that is not a member;</li>
  <li>a duplicate key; a dependency naming both a <code>git</code> and a <code>path</code>, or no source at all;</li>
  <li>an optional dev-dependency; a version requirement with no number in it.</li>
</ul>
<Callout variant="info" title="Severity means something">
  Red is only for what Cargo refuses to build. An unknown key is a warning — Cargo warns on those and gains new ones every few releases. Nothing is said about a table
  Bennu does not recognise, or anything under <code>[package.metadata]</code> and <code>[lints.*]</code>, which belong to other tools.
</Callout>

<h2>Dependencies</h2>
<p>
  <kbd>Alt</kbd> + <kbd>N</kbd> opens the panel a Maven project uses, in Cargo's vocabulary: one group per crate, and per row the crate, the version,
  <strong>where that version came from</strong>, its kind — <code>normal</code>, <code>dev</code>, <code>build</code> — and whether it is in the local registry.
</p>
<ul>
  <li>The version is the one <code>Cargo.lock</code> chose, not the requirement written — <code>serde = "1"</code> is not what you compile against. With no lockfile
    the panel says so and shows the requirement, <em>unknown</em> rather than <em>missing</em>.</li>
  <li>A <code>workspace = true</code> dependency is marked as pinned by the workspace; a renamed one shows both names; a target-specific one carries its
    <code>cfg(…)</code>.</li>
  <li>The last group is everything in the lockfile no crate of yours declares — what your dependencies dragged in.</li>
</ul>

<h2>Running and debugging</h2>
<p>
  ▶ and <kbd>Shift</kbd> + <kbd>F10</kbd> launch the active run configuration, and a <strong>Cargo</strong> configuration is a cargo subcommand: a crate, a command, a
  target, features, a profile, and two argument fields. On a workspace with exactly one binary none of it is needed — ▶ makes the configuration and runs it. With several
  it opens the editor, because <code>cargo run</code> refuses to guess too.
</p>
<table>
  <thead><tr><th>Field</th><th>Goes</th><th>Reaches</th></tr></thead>
  <tbody>
    <tr><td><strong>Cargo arguments</strong></td><td>Before the <code>--</code></td><td>cargo</td></tr>
    <tr><td><strong>Program arguments</strong></td><td>After it</td><td>Your program or the test harness — <code>--nocapture</code> belongs here</td></tr>
  </tbody>
</table>
<p>
  A cargo run has no build step in front of it — the command <em>is</em> the build. Its output, Stop, ⟳ and the tab strip are the Run console's usual ones. Debugging a
  Cargo configuration builds the binary first and launches it under a debug adapter — see <strong>Debugging</strong>.
</p>

<h2>Navigating</h2>
<p>
  <kbd>Ctrl</kbd> + <kbd>N</kbd> opens the navigator on <strong>Types</strong> rather than Classes — structs, enums, traits and type aliases — and
  <kbd>Ctrl</kbd> + <kbd>Shift</kbd> + <kbd>Y</kbd> finds functions, methods and constants. The language server does the matching, so both start at two characters.
</p>
<Callout variant="info" title="No Trees panel here">
  Both of its views read Bennu's own engines — tree-sitter grammars for Java and JSP, and Java's declaration model — so on a Rust project it could only report their
  absence, which would read as Bennu not understanding a language rust-analyzer is answering everything about. The rail slot holds the Cargo window instead.
</Callout>
