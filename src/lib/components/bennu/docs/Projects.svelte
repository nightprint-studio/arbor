<!-- Bennu docs — Projects, JDK & capabilities. -->
<h1>Projects, JDK &amp; capabilities</h1>
<p class="doc-lead">
  A Bennu project is the folder holding a root manifest — a <strong>Maven</strong>
  <code>pom.xml</code> or a <strong>Cargo</strong> <code>Cargo.toml</code>. Opening it resolves the
  build model: the display name, the modules or workspace crates, and — for Maven — the JDK language
  level and the domain frameworks the code relies on.
</p>

<h2>Maven and Cargo</h2>
<p>
  <strong>Maven</strong> projects get the whole of Bennu: the symbol index, completion, go-to
  declaration, find usages, rename, capability detection, JDK resolution, validation, Generate, the
  Structure / Maven / Dependencies / Services / Forms tool windows, and Tomcat hot-swap.
</p>
<p>
  <strong>Cargo</strong> projects get the <strong>editor</strong>: the file tree, go-to file,
  find in files, TODOs, the terminal, Rust and TOML highlighting, and
  <strong>Check project</strong> — <code>cargo check</code> over the workspace, whose errors and
  warnings land in the Problems panel and on the editor gutter like any other build. What is
  <em>not</em> there is everything that would need a Rust symbol index: completion, navigation,
  rename. Those need a language server, and until one is wired the actions are hidden rather than
  offered and silent. The Java-only tool windows and the JDK footer are hidden too, so the window
  never shows a panel that can only ever be empty.
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
  workspaces and add or remove their member projects. <strong>Open project</strong>
  (<kbd>Ctrl</kbd> + <kbd>O</kbd>) resets the active workspace to a single project; the whole set is
  remembered and reopened on the next launch. <strong>Find in project</strong> gains a scope toggle
  to search the active project only or the <strong>entire workspace</strong>.
</p>

<h2>New files</h2>
<p>
  The Project tree's <strong>＋</strong> button — or <strong>New file…</strong> from a right-click
  (or the tool-window menu) — scaffolds a file in the chosen directory: a Java
  <strong>class / interface / enum / record / annotation</strong> (the <code>package</code> is
  <strong>inferred</strong> from the directory, following <code>src/main/java</code> and friends), a
  <strong>JSP</strong> or <strong>XML</strong> file with the right header, or a plain file. It opens
  the new file and reveals it in the tree; it never overwrites an existing one.
</p>

<h2>The JDK</h2>
<p>
  The footer shows the resolved Java language level and where it came from — usually
  <code>maven.compiler.source</code>, but also <code>maven.compiler.release</code>,
  <code>&lt;java.version&gt;</code>, the compiler plugin, toolchains, or a manual override. When it
  can't be inferred the footer reads <code>JDK —</code>.
</p>
<p>
  The <em>install</em> Bennu resolves the standard library against is looked for in the extra JDK
  directories from Settings first, then <code>JAVA_HOME</code>, then each platform's usual
  locations: the <code>JavaVirtualMachines</code> bundles on macOS, the Program Files vendor
  directories on Windows, <code>/usr/lib/jvm</code> on Linux, the Homebrew <code>openjdk</code>
  formula, and the directories a version manager or an IDE installs JDKs into. The one whose level
  matches the project wins; failing that, the newest installed. When none is found the title bar
  carries a <strong>No JDK</strong> warning, because without one nothing — not even
  <code>String</code> — resolves.
</p>

<h2>Dependencies</h2>
<p>
  For a Maven project Bennu resolves the dependency jars from your local repository so completion,
  navigation and validation see library types, not just the JDK and your own sources. The resolve is
  <strong>offline</strong>: a dependency that has never been downloaded can't be resolved, so build
  the project once. Every module of a multi-module project contributes its own dependencies.
</p>
<p>
  The result is cached against your poms' timestamps — editing a <code>pom.xml</code> re-resolves,
  and <strong>Rebuild index</strong> re-resolves unconditionally. When the resolve can't happen at
  all, Bennu says so with the reason rather than leaving you with unresolvable library types: Maven
  is looked for on <code>PATH</code>, then in the usual install directories, then as the project's
  own <code>mvnw</code> wrapper.
</p>

<h2>Capabilities</h2>
<p>
  Bennu detects the domain frameworks a project uses and shows the count in the footer. Each
  capability is backed by <strong>evidence</strong> at one of three tiers:
</p>
<ul>
  <li><strong>Tier A</strong> — a dependency coordinate in the <code>pom.xml</code> (strongest).</li>
  <li><strong>Tier B</strong> — a configuration file (e.g. a <code>struts.xml</code> or a TLD).</li>
  <li><strong>Tier C</strong> — a source pattern (corroborating; a C-only hit is provisional).</li>
</ul>
<p>
  The detected set gates which features light up — for example, JSP taglib awareness only when a
  taglib is actually in use. The demo project shows Struts (convention + XML), JSP taglibs, the
  OGNL value stack, a JDBC DAO and Entando.
</p>

<h2>Form analysis</h2>
<p>
  The <strong>Forms</strong> tool window (bottom dock, toggled from the right rail with
  <kbd>Alt</kbd> + <kbd>3</kbd> — offered only on a project that actually has JSP pages) analyses
  the open JSP and lists every <code>&lt;form&gt;</code>
  relevant to it with the <strong>complete set of parameters</strong> it posts. It resolves the action each form targets — the mapped action class
  and the <code>struts.xml</code> fragment that declares it, even when the target is written as an
  Entando <code>&lt;wp:action path=…&gt;</code> — and lists every input, including
  <strong>hidden</strong> ones, with the <code>value</code> each posts (a fixed value or an
  <code>$&lbrace;…&rbrace;</code>/<code>%&lbrace;…&rbrace;</code> expression). A field inside a
  <code>&lt;c:if&gt;</code>/<code>&lt;s:if&gt;</code> is marked <strong>if</strong> (hover for the
  condition) since it is submitted only when that test holds. Two badges flag each field:
  <strong>bound</strong> when the field name is a writable property of the action class, and
  <strong>valid</strong> when it carries a Struts validation rule. A field that is neither reads as
  muted — the signal a name is a typo or an unmapped request parameter. Clicking a form or field
  jumps the editor to it; the config button opens the declaring fragment.
</p>
<p>
  It is <strong>include-aware</strong>. A JSP form is often split across
  <code>&lt;jsp:include&gt;</code>s — the page opens the <code>&lt;form&gt;</code> and the hidden
  tokens, wizard-step inputs and button bar come from included fragments. So each form gathers those
  fields too: on a parent page you see all the parameters, the children's included, each tagged with
  the fragment it comes from. And the reverse — when you are on an included fragment, the parent form
  it feeds surfaces (a chip names the page it lives on) with its whole parameter set, and the fields
  <em>this</em> fragment contributes are highlighted. The walk is recursive and cycle-safe; a very
  large include graph shows a "…more" hint rather than silently dropping pages.
</p>

<h2>Encodings</h2>
<p>
  Legacy projects often declare <code>Cp1252</code> in their <code>pom.xml</code>
  (<code>project.build.sourceEncoding</code>). Bennu decodes each file with the pom-declared
  encoding and shows which one won in the footer, so a mojibake surprise never slips in silently.
</p>
<p>
  A <strong>Cargo</strong> project is always <code>UTF-8</code>: Rust source is UTF-8 by language
  definition, so the encoding default configured for a legacy Java tree never reaches it.
</p>

<h2>Tomcat hot-swap</h2>
<p>
  Link the project to a local Tomcat and push changed JSPs straight into the running server — no
  redeploy, no restart. Open <strong>Tomcat hot-swap…</strong> from the command palette and pick the
  Tomcat root (the folder holding <code>webapps/</code>, <code>bin/</code>, <code>conf/</code>).
  Bennu validates it, lists the deployed web applications, and auto-selects the one this project maps
  to — by <code>&lt;finalName&gt;</code>, artifactId or folder name, or the only app deployed. The
  link is remembered per project.
</p>
<p>
  <strong>Deploy current JSP</strong> (<kbd>Ctrl</kbd> + <kbd>Shift</kbd> + <kbd>F10</kbd>) saves the
  open JSP and copies it into the deployed webapp at the same path it has under the project's webapp
  source dir; Tomcat recompiles it on the next request. <strong>Deploy all JSPs to Tomcat</strong>
  (command palette) copies every page at once. Files are copied byte-for-byte, so each JSP keeps its
  own page encoding. A toast confirms what was deployed, or explains why it couldn't be (no Tomcat
  linked, no webapp source dir, or an ambiguous target — pick one in the settings).
</p>
