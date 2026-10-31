<script lang="ts">
  /**
   * The Gradle tool window: the build's projects as things to run, what a press actually runs, the
   * switches that shape every run, and what Bennu reads of a Gradle build (and what it leaves to Gradle).
   */
  import Callout from '$lib/components/shared/ui/Callout.svelte';
</script>

<span class="eyebrow">Projects</span>
<h1>The Gradle tool window</h1>

<p class="doc-lead">
  The build's own scripts, as things to run. <kbd>Alt</kbd> + <kbd>8</kbd> opens it. Its rail button is there whenever the
  workspace has a Gradle build, in the slot Maven's takes on a Maven project. A project with a
  <code>settings.gradle[.kts]</code> or a <code>build.gradle[.kts]</code> at its root opens as a Java project, with the same
  symbol index, completion and validation a Maven one gets.
</p>

<h2>One section per project</h2>
<p>
  A Gradle build is a root plus the projects <code>settings.gradle</code> includes — <code>include ':api', ':web:client'</code>,
  with <code>project(':x').projectDir</code> honoured. The window has one section per project, the root first, and each holds
  two groups:
</p>
<div class="feature-grid two-col">
  <div class="feature-card">
    <div class="fc-eyebrow">What the project offers</div>
    <div class="fc-title">Tasks</div>
    <div class="fc-desc">
      The tasks the applied plugins are known to add (<code>build</code>, <code>test</code>, <code>run</code>,
      <code>bootRun</code>, <code>war</code>…) plus the ones the script declares itself with
      <code>tasks.register</code> or <code>task</code>. <code>clean build</code> is there as one row.
    </div>
  </div>
  <div class="feature-card">
    <div class="fc-eyebrow">What the script applies</div>
    <div class="fc-title">Plugins</div>
    <div class="fc-desc">
      From <code>plugins {'{'} … {'}'}</code> and <code>apply plugin:</code>. A row opens the build script at the line that
      applies it; <code>apply false</code> is labelled.
    </div>
  </div>
</div>
<p>
  A <strong>Version catalogs</strong> section lists <code>gradle/libs.versions.toml</code> and any catalog the settings declare;
  a row opens the file. Which sections are open is remembered for each project.
</p>

<h2>What a press runs</h2>
<p>
  A task row runs that task <strong>in that project</strong> — <code>test</code> on <code>:orders</code> runs
  <code>:orders:test</code> from the build root. The root row runs the bare task name, which Gradle applies to every project that
  has it: the whole build. The output goes to the Run console, in a tab named for the task and the project, and ⟳ on the tab
  repeats exactly that run.
</p>
<p>
  The launcher is the project's own <code>gradlew</code> when it has one — a build is only guaranteed to work with the Gradle
  version it pins — else a <code>gradle</code> on the <code>PATH</code> or in the usual install directories.
</p>

<h2>The switches</h2>
<table class="doc-table">
  <thead><tr><th>Control</th><th>What it does</th></tr></thead>
  <tbody>
    <tr><td><strong>Skip tests</strong> (flask)</td><td>Adds <code>-x test</code> to every task run from the window, until turned off.</td></tr>
    <tr><td><strong>Offline</strong> (actions menu)</td><td><code>--offline</code> on every task and on the classpath resolve. The same setting as <em>Settings → Java</em>.</td></tr>
    <tr>
      <td><strong>Re-resolve dependencies</strong></td>
      <td rowspan="2">Under the <strong>actions</strong> menu: these change what is on disk, as background jobs that report in the Jobs panel.</td>
    </tr>
    <tr><td><strong>Download missing dependencies</strong> <kbd>Alt</kbd> + <kbd>Shift</kbd> + <kbd>U</kbd></td></tr>
    <tr>
      <td><strong>Module graph</strong> <kbd>Alt</kbd> + <kbd>Shift</kbd> + <kbd>D</kbd></td>
      <td>This window lists the projects; the graph shows how <code>project(':x')</code> dependencies wire them together.</td>
    </tr>
  </tbody>
</table>

<h2>Dependencies</h2>
<p>
  The Dependencies window reads a Gradle build the way it reads a pom: one group per project, each row with the
  <strong>scope</strong> its configuration folds into (<code>implementation</code> and <code>api</code> are <em>compile</em>,
  <code>compileOnly</code> is <em>provided</em>, anything with <em>test</em> in it is <em>test</em>), the version, <strong>who decided
  that version</strong> — the script, a variable (<code>ext</code>, <code>gradle.properties</code>), a version-catalog alias, a
  BOM, or an <code>allprojects {'{'} {'}'}</code> / <code>subprojects {'{'} {'}'}</code> block of a parent — and whether the jar is in the
  Gradle cache. Every row jumps to the line of the build script that declares it. <strong>Resolve tree</strong> runs
  <code>gradle dependencies</code> in the background and adds who brings each dependency in, and the versions Gradle replaced
  (<code>1.0 -&gt; 1.2</code>) as conflicts.
</p>

<h2>What it reads, and what it does not</h2>
<p>
  Building the window costs a read of the settings and every build script — no Gradle, no network — so it draws at once on a
  build that was never run, and the refresh is cheap enough to press freely. Gradle starts only when you press a task, resolve
  the tree, or when the symbol index asks it for the classpath (a few seconds on a warm daemon; the result is cached until a
  script, <code>libs.versions.toml</code> or <code>gradle.properties</code> changes).
</p>
<Callout variant="info" title="Scripts are read, not run">
  Groovy and Kotlin build scripts are programs. Bennu reads the declarative subset every real build is made of — plugins,
  dependency blocks, variables, <code>sourceCompatibility</code>, <code>java {'{'} toolchain {'}'}</code>, <code>options.encoding</code> — and
  nothing else. A dependency added in a loop, by a function or by a convention plugin does not appear in the Dependencies list
  (the resolved tree and the index classpath, which ask Gradle, do include it), and a convention plugin's tasks are not in the
  task list: run <code>gradle tasks</code> in a terminal for the authoritative one.
</Callout>
<Callout variant="info" title="Running a main class">
  The ▷ button next to a <code>main</code> method launches a JVM on a classpath assembled from a Maven reactor, which a Gradle
  build does not have. For now, run the application from the window — <code>run</code>, <code>bootRun</code> — and use
  the Build button (<code>gradle classes</code>) for diagnostics.
</Callout>
<p>
  When no Gradle can be found — no <code>gradlew</code> in the project and no <code>gradle</code> on the <code>PATH</code> this
  app inherits — the window says so once at the top. The index then falls back to the jars the build files declare, read straight
  from the Gradle cache, and says that its classpath is partial.
</p>
