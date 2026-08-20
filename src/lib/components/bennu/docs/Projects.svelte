<!-- Bennu docs — what a project is, how it is resolved, and where its settings live. -->
<h1>Projects</h1>
<p class="doc-lead">
  A Bennu project is the folder holding a root manifest — a <strong>Maven</strong>
  <code>pom.xml</code> or a <strong>Cargo</strong> <code>Cargo.toml</code>. Opening it resolves the
  build model: the display name, the modules or workspace crates, and — for Maven — the JDK
  language level and the domain frameworks the code relies on.
</p>

<h2>Maven and Cargo</h2>
<p>
  <strong>Maven</strong> projects get the whole of Bennu: the symbol index, completion, go-to
  declaration, find usages, rename, capability detection, JDK resolution, validation, Generate, the
  Structure / Maven / Dependencies / Forms tool windows, and Tomcat hot-swap.
</p>
<p>
  <strong>Cargo</strong> projects get the editor plus everything
  <a href="#lsp"><strong>rust-analyzer</strong></a> supplies: completion, go-to declaration, find
  usages, hover, rename, quick fixes, <code>rustfmt</code>, semantic colouring, and the compiler's
  own diagnostics on save. On top of that the shared surface — the file tree, go-to file, find in
  files, TODOs, the terminal, TOML highlighting — and <strong>Check project</strong>
  (<code>cargo check</code> over the workspace, whose errors land in the Problems panel like any
  other build).
</p>
<p>
  The intelligence comes from the language server, so it depends on rust-analyzer being installed;
  the footer says which server is serving the open file and whether it is ready. What stays hidden
  on a Cargo project is the Java-specific machinery — the JDK footer, the capability count, the
  Structure / Maven / Dependencies / Forms tool windows — so the window never shows a panel that
  can only ever be empty. See <strong>Language servers</strong> for the whole picture.
</p>
<p>
  A folder holding <em>both</em> manifests opens as the Maven project: it is the model that has more
  to say. The footer names which kind is open — the JDK and capability count for Maven, the
  toolchain and crate count for Cargo.
</p>
<h2>Workspaces</h2>
<p>
  A <strong>workspace</strong> is a named, colored group of projects you can switch between as a
  unit. Hold several projects in one workspace and keep several workspaces side by side — the same
  project may belong to more than one. Each workspace remembers its own open tabs, so switching
  reopens exactly where you left off.
</p>
<p>
  <strong>There is always one.</strong> A default workspace called <em>Scratch</em> exists from the
  start, so a project has somewhere to land without you creating anything first, and deleting the
  last workspace leaves it rather than leaving nothing. A workspace you create without naming takes
  the name of the first project you add to it.
</p>
<p>
  The <strong>switcher</strong> in the titlebar is a tree: every workspace is a row, its member
  projects nested underneath. Click a <strong>workspace</strong> to switch to it, or a
  <strong>project</strong> to jump straight into it (switching workspace first if needed). Every
  project keeps its tabs, tree and index in memory, so switching — a project or a whole workspace —
  never reopens anything. A file opened from a different project of the workspace stays in the
  current tab strip, <strong>badged with its owning project</strong>. From the command palette,
  <strong>Switch project</strong> and <strong>Switch workspace</strong> do the same from the
  keyboard.
</p>
<p>
  The <strong>workspace manager</strong> (<kbd>Ctrl</kbd> + <kbd>Shift</kbd> + <kbd>W</kbd>, or
  <em>Manage workspaces…</em> in the dropdown) is where you create, rename, recolor and delete
  workspaces and add or remove their member projects; <em>Add project…</em> in the switcher adds one
  to the workspace you are in, whether or not it already holds any. <strong>Open project</strong>
  (<kbd>Ctrl</kbd> + <kbd>O</kbd>) resets the active workspace to a single project; the whole set is
  remembered and reopened on the next launch, and closing the window writes the last of it before it
  goes. <strong>Find in project</strong> and <strong>Go
  to</strong> both gain a toggle beside their field that reaches into every member project at
  once, and a row from another one says which.
</p>
<h2>Which profile, and where it is kept</h2>
<p>
  A <strong>profile</strong> is an isolated Arbor environment — its own settings, plugins and, for
  Bennu, its own workspaces. The gear menu's <strong>Profile</strong> submenu names the active one
  and switches between them; <em>Manage profiles…</em> creates, clones, renames and deletes them.
  Switching is live: the window reloads onto the new profile and Bennu's backend is restarted so it
  reads and writes the new one's files rather than the old one's.
</p>
<p>Bennu keeps two files, both inside the active profile:</p>
<ul>
  <li><code>bennu/config.toml</code> — the settings (editor toggles, JDK paths, language servers);</li>
  <li><code>bennu/workspace.toml</code> — the workspaces, their projects and their open tabs.</li>
</ul>
<p>
  The profile folder lives under Arbor's config root:
  <code>~/Library/Application Support/arbor/profiles/&lt;profile&gt;/</code> on macOS,
  <code>%APPDATA%\arbor\profiles\&lt;profile&gt;\</code> on Windows,
  <code>~/.config/arbor/profiles/&lt;profile&gt;/</code> on Linux. The heavy things Bennu builds —
  the symbol indices and cached decompiled sources — are deliberately <em>outside</em> it, under
  <code>arbor/data/bennu/</code>, so an index built once is shared by every profile instead of being
  rebuilt per profile.
</p>
<p>
  A development build runs on the <code>dev</code> profile by default and an installed one on
  <code>default</code>, each tracking its own selection, so running from source never touches an
  installed Arbor's data.
</p>
<h2>Capabilities</h2>
<p>
  Bennu detects the domain frameworks a project uses and shows the count in the footer. Each
  capability is backed by <strong>evidence</strong> at one of three tiers:
</p>
<ul>
  <li><strong>Tier A</strong> — a declared dependency: a coordinate in the <code>pom.xml</code>, or a
    crate in a <code>Cargo.toml</code> (strongest).</li>
  <li><strong>Tier B</strong> — a configuration file (e.g. a <code>struts.xml</code> or a TLD).</li>
  <li><strong>Tier C</strong> — a source pattern (corroborating; a C-only hit is provisional).</li>
</ul>
<p>
  The detected set gates which features light up — for example, JSP taglib awareness only when a
  taglib is actually in use. The demo project shows Struts (convention + XML), JSP taglibs, the
  OGNL value stack, a JDBC DAO and Entando.
</p>
<p>
  Not every capability is Java's. A Cargo project is detected the same way, from the same evidence
  in its own manifest: a <code>bevy</code> (or <code>bevy_*</code>) dependency turns on the
  <strong>Bevy ECS</strong> tooling, and an <code>i18n/languages.toml</code> beside a
  <code>.ron</code> tree turns on the <strong>i18n labels</strong> one — that second by layout
  rather than by dependency, because it is useful on a project that only authors content.
</p>
