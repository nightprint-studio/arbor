<script lang="ts">
  /**
   * Projects: what Bennu opens, what it reads from the manifest, workspaces, where its files live, and
   * capabilities. Whether a project is Maven or Cargo decides most of what the rest of the manual applies to,
   * so that comes first.
   */
  import Callout from '$lib/components/shared/ui/Callout.svelte';
</script>

<span class="eyebrow">Projects</span>
<h1>Projects</h1>

<p class="doc-lead">
  A Bennu project is a folder holding a root manifest — a <strong>Maven</strong> <code>pom.xml</code> or a
  <strong>Cargo</strong> <code>Cargo.toml</code>. Opening it reads the build model: what the project is called,
  its modules or workspace crates, and — for Maven — the JDK language level and the frameworks the code relies on.
</p>

<h2>What opening a folder does</h2>
<ol class="step-list">
  <li>Bennu reads the root manifest: the display name, and the modules or the workspace crates.</li>
  <li>On a Maven project it resolves the <strong>JDK language level</strong> and detects the
    <strong>capabilities</strong> — the domain frameworks the code uses.</li>
  <li>The footer says which kind of project is open: the JDK and the capability count for Maven, the toolchain
    and the crate count for Cargo.</li>
</ol>

<h2>The model follows the manifest</h2>
<p>
  Saving a <code>pom.xml</code> or a <code>Cargo.toml</code> reads it again. Rename an
  <code>&lt;artifactId&gt;</code> and the project is renamed everywhere it is shown — the window title, the
  workspace switcher, the Canopy recents — without closing anything.
</p>
<Callout variant="info" title="Only the model is read again">
  The index is not rebuilt and no language server is restarted.
</Callout>

<h2>Maven and Cargo</h2>
<p>The two kinds of project get different tools, because different engines answer about them.</p>
<div class="feature-grid two-col">
  <div class="feature-card">
    <div class="fc-eyebrow"><code>pom.xml</code></div>
    <div class="fc-title">Maven — all of Bennu</div>
    <div class="fc-desc">
      The symbol index, completion, go-to declaration, find usages, rename, capability detection, JDK resolution,
      validation, Generate, the Structure, Maven, Dependencies and Forms tool windows, and Tomcat hot-swap.
    </div>
  </div>
  <div class="feature-card">
    <div class="fc-eyebrow"><code>Cargo.toml</code></div>
    <div class="fc-title">Cargo — the editor and rust-analyzer</div>
    <div class="fc-desc">
      Everything <a href="#lsp"><strong>rust-analyzer</strong></a> supplies: completion, go-to declaration, find
      usages, hover, rename, quick fixes, <code>rustfmt</code>, semantic colouring and the compiler's own diagnostics
      on save.
    </div>
  </div>
</div>
<p>
  A Cargo project also gets the shared surface — the file tree, go-to file, find in files, TODOs, the terminal,
  TOML highlighting — and <strong>Check project</strong>, which runs <code>cargo check</code> over the workspace and
  puts its errors in the Problems panel like any other build.
</p>
<p>
  Its intelligence comes from the language server, so it depends on rust-analyzer being installed; the footer says
  which server is serving the open file and whether it is ready. What stays hidden on a Cargo project is the
  Java-specific machinery — the JDK footer, the capability count, the Structure, Maven, Dependencies and Forms tool
  windows — so the window never shows a panel that could only ever be empty. <strong>Language servers</strong> has
  the whole picture.
</p>
<Callout variant="tip" title="A folder with both manifests">
  It opens as the Maven project: that is the model with more to say.
</Callout>

<h2>Workspaces</h2>
<p>
  A <strong>workspace</strong> is a named, coloured group of projects you switch between as a unit. One workspace
  holds several projects, several workspaces sit side by side, and the same project may belong to more than one.
</p>
<p>
  Each workspace remembers its open tabs — and the line each of them was on — so switching, or opening Bennu again
  tomorrow, puts you back where you were rather than at the top of every file.
</p>
<Callout variant="info" title="There is always one">
  A default workspace called <em>Scratch</em> exists from the start, so a project has somewhere to land without you
  creating anything first; deleting the last workspace leaves it rather than leaving nothing. A workspace you create
  without naming takes the name of the first project you add to it.
</Callout>
<dl class="meta-grid">
  <dt>The switcher</dt>
  <dd>
    In the title bar, as a tree: every workspace is a row with its projects nested underneath. Click a
    <strong>workspace</strong> to switch to it, or a <strong>project</strong> to jump straight into it — switching
    workspace first if needed. <em>Add project…</em> adds one to the workspace you are in, whether or not it already
    holds any. From the command palette, <strong>Switch project</strong> and <strong>Switch workspace</strong> do the
    same from the keyboard.
  </dd>
  <dt>The workspace manager</dt>
  <dd>
    <kbd>Ctrl</kbd> + <kbd>Shift</kbd> + <kbd>W</kbd>, or <em>Manage workspaces…</em> in the dropdown: create, rename,
    recolour and delete workspaces, and add or remove their projects.
  </dd>
  <dt>Open project</dt>
  <dd>
    <kbd>Ctrl</kbd> + <kbd>O</kbd> resets the active workspace to a single project. The whole set is remembered and
    opened again on the next launch — closing the window writes the last of it before it goes.
  </dd>
  <dt>Across its projects</dt>
  <dd>
    <strong>Find in project</strong> and <strong>Go to</strong> both have a toggle beside their field that reaches into
    every project of the workspace at once, and a row from another project says which.
  </dd>
</dl>
<p>
  Every project keeps its tabs, tree and index in memory, so switching — a project or a whole workspace — never opens
  anything again. A file opened from another project of the workspace stays in the current tab strip,
  <strong>badged with the project it belongs to</strong>.
</p>

<h2>Which profile, and where it is kept</h2>
<p>
  A <strong>profile</strong> is an isolated Arbor environment — its own settings, plugins and, for Bennu, its own
  workspaces. The gear menu's <strong>Profile</strong> submenu names the active one and switches between them;
  <em>Manage profiles…</em> creates, clones, renames and deletes them. Switching is live: the window reloads onto the
  new profile, and Bennu's backend restarts so it reads and writes the new profile's files instead of the old one's.
</p>
<p>Bennu keeps two files, both inside the active profile:</p>
<dl class="meta-grid">
  <dt><code>bennu/config.toml</code></dt>
  <dd>The settings — editor toggles, JDK paths, language servers.</dd>
  <dt><code>bennu/workspace.toml</code></dt>
  <dd>The workspaces, their projects and their open tabs.</dd>
</dl>
<table>
  <thead><tr><th>System</th><th>The profile folder</th></tr></thead>
  <tbody>
    <tr><td>macOS</td><td><code>~/Library/Application Support/arbor/profiles/&lt;profile&gt;/</code></td></tr>
    <tr><td>Windows</td><td><code>%APPDATA%\arbor\profiles\&lt;profile&gt;\</code></td></tr>
    <tr><td>Linux</td><td><code>~/.config/arbor/profiles/&lt;profile&gt;/</code></td></tr>
  </tbody>
</table>
<Callout variant="info" title="The heavy things are shared">
  The symbol indices and the cached decompiled sources are deliberately <em>outside</em> the profile, under
  <code>arbor/data/bennu/</code>, so an index built once serves every profile instead of being built again for each.
</Callout>
<p>
  A development build runs on the <code>dev</code> profile by default and an installed one on <code>default</code>,
  each remembering its own choice, so running from source never touches an installed Arbor's data.
</p>

<h2>Capabilities</h2>
<p>
  Bennu detects the domain frameworks a project uses and shows how many in the footer. Each capability rests on
  <strong>evidence</strong>, at one of three tiers:
</p>
<table>
  <thead><tr><th>Tier</th><th>Evidence</th><th>Weight</th></tr></thead>
  <tbody>
    <tr>
      <td><strong>A</strong></td>
      <td>A dependency: a coordinate in any <code>pom.xml</code> of the reactor, one the build resolves to transitively,
        or a crate in a <code>Cargo.toml</code>.</td>
      <td>The strongest</td>
    </tr>
    <tr>
      <td><strong>B</strong></td>
      <td>A configuration file — a <code>struts.xml</code>, a TLD — in any module.</td>
      <td>Strong</td>
    </tr>
    <tr>
      <td><strong>C</strong></td>
      <td>A pattern in the source.</td>
      <td>Corroborating; a capability with only this is provisional</td>
    </tr>
  </tbody>
</table>
<p>
  The resolved dependency tree matters more than it sounds, because an API is rarely what a pom declares.
  <code>jakarta.validation-api</code> arrives through a Spring starter or a parent pom, so Bean Validation is
  recognised from the tree once Maven has resolved it — and the tooling that depends on it switches on then, without
  opening the project again.
</p>
<p>
  The detected set decides which features light up — JSP taglib awareness only when a taglib is actually in use, for
  instance. The demo project shows Struts (convention and XML), JSP taglibs, the OGNL value stack, a JDBC DAO and
  Entando.
</p>
<p>
  <strong>Project Configuration</strong> — the title bar's gear, or the command palette — lists what was detected here
  with the evidence for each, beside the rest of what belongs to this project: the JDK level it targets, its encoding,
  its naming rules and its roots. Settings holds what belongs to you and to the machine instead, which is why the two
  are different dialogs.
</p>
<div class="feature-grid two-col">
  <div class="feature-card">
    <div class="fc-eyebrow">A <code>bevy</code> or <code>bevy_*</code> dependency</div>
    <div class="fc-title">Bevy ECS</div>
    <div class="fc-desc">A Cargo project is detected the same way, from the same evidence in its own manifest.</div>
  </div>
  <div class="feature-card">
    <div class="fc-eyebrow"><code>i18n/languages.toml</code> beside a <code>.ron</code> tree</div>
    <div class="fc-title">i18n labels</div>
    <div class="fc-desc">Recognised by layout rather than by dependency, because it is useful on a project that only authors content.</div>
  </div>
</div>
