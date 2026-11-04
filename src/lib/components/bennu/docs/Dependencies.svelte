<script lang="ts">
  /**
   * Dependencies, overview: how the classpath is resolved without a build, the actions that change what is on disk,
   * the tool window that lists it, and what it will not compute. Versions and vulnerabilities have their own pages.
   */
  import Callout from '$lib/components/shared/ui/Callout.svelte';
</script>

<span class="eyebrow">Projects</span>
<h1>Dependencies</h1>

<p class="doc-lead">
  The libraries a project pulls in — listed, searchable and, with sources, readable in the same editor as your own code.
</p>
<p>
  The panel describes one Maven, Gradle, Cargo or Node project at a time. When the workspace has several (a Rust crate in a Node project's subfolder, two
  projects side by side), a tree of them tops the panel and picks which; with one there is nothing to pick.
</p>

<h2>Node projects</h2>
<p>
  A folder with a <code>package.json</code> is a Node project — npm, pnpm or Yarn, told apart by the lockfile beside it. The
  panel lists one group per <code>package.json</code>: the root, and every member of a monorepo's workspace
  (<code>workspaces</code> in the manifest, or <code>pnpm-workspace.yaml</code>). Each row is a declared dependency, with:
</p>
<ul class="prop-list">
  <li><strong>The version you actually get</strong>from <code>package-lock.json</code>, <code>pnpm-lock.yaml</code> or <code>yarn.lock</code> — the range in the manifest (<code>^4.17.0</code>) is only shown when there is no lockfile yet</li>
  <li><strong>The kind</strong><code>runtime</code>, <code>dev</code>, <code>peer</code> or <code>optional</code> — the section of the manifest it is declared in</li>
  <li><strong>Where it comes from</strong>a tag for anything that is not the registry: <code>workspace</code> (a sibling package, shown at its own version), <code>git</code>, <code>file</code>, <code>url</code>, an <code>npm:</code> <code>alias</code> or a pnpm <code>catalog</code></li>
  <li><strong>Whether it is installed</strong>a declared package with no <code>node_modules</code> folder (found the way Node finds it, up through the parent folders) is called out as <em>not installed</em>; peer and optional ones are exempt, since they are legitimately absent. Until there is a lockfile or a <code>node_modules</code>, the panel says the column is unknown</li>
  <li><strong>A newer release</strong>an <code>↑ 5.0.1</code> badge, amber for a major bump, on a package whose resolved version is behind the registry's <code>latest</code>. It uses the same <em>look packages up online</em> switch and cache window as the version hints inside <code>package.json</code>, so turning that off removes the badges</li>
  <li><strong>Vulnerabilities</strong>the shield badge — see <strong>Dependency security</strong></li>
</ul>
<p>
  <strong>Resolve tree</strong> on a Node project reads the lockfile — no <code>npm ls</code>, no network, no package manager — and
  runs as a row in the Jobs panel. It gives the <strong>Duplicate versions</strong> group (a package present at several versions, with
  who asked for each), the <em>via …</em> line on every transitive row and the full routes down to it. Without a lockfile there is no
  resolved graph to read, and the panel says to install first.
</p>
<p>
  The <strong>Node</strong> tool window (right rail, <kbd>Alt</kbd> + <kbd>Shift</kbd> + <kbd>9</kbd>) is the other half of a
  <code>package.json</code>: each package's <code>scripts</code> as rows that run in the Run console with the project's package
  manager, and <strong>Install</strong> (<code>npm install</code>, <code>pnpm install</code> or <code>yarn install</code>). Above the scripts a
  <strong>Dependencies</strong> row summarises the project — declared and transitive counts, how many packages have a newer release, how many are
  vulnerable — and opens the Dependencies panel on it; the toolbar also opens the Vulnerabilities window. A project that is only a frontend gets the Dependencies panel and the Node window
  but no Tests catalogue and no <kbd>Alt</kbd> + <kbd>8</kbd> build-tool slot.
</p>

<h2>How the classpath is found</h2>
<p>
  On a Maven project Bennu resolves the dependency jars from your local repository, so completion, navigation and
  validation see library types and not only the JDK and your own sources. Every module of a multi-module project adds its
  own.
</p>
<ol class="step-list">
  <li>The poms are read straight from disk with your local repository — parent chains, imported BOMs, the transitive
    closure and the exclusions — in milliseconds and with no build tool involved.</li>
  <li>Only when that comes up short is Maven run — and the two answers are combined, because a <code>mvn</code> run that
    fails halfway still wrote the entries it did resolve.</li>
  <li>The result is cached against your poms' timestamps, against what was missing, <em>and</em> against the library
    files it was resolved from — a jar or pom in your local repository that is rewritten retires it.</li>
</ol>
<Callout variant="info" title="A broken Maven still gets a classpath">
  A project whose Maven is missing, misconfigured or simply broken still resolves, from the poms and the repository alone.
</Callout>
<p>
  The resolve is <strong>offline</strong> — it downloads nothing — so a dependency that has never been fetched cannot be
  resolved, and Bennu names it rather than reporting a count. The moment one of the missing artifacts lands in your
  repository the cache is stale, so installing a dependency is picked up on the next open instead of being hidden until a
  pom is edited. <strong>Rebuild index</strong> resolves again unconditionally.
</p>
<p>
  When Maven is needed it is looked for on <code>PATH</code>, then in the usual install directories, then as the
  project's own <code>mvnw</code> wrapper.
</p>
<Callout variant="tip" title="Saving a pom is enough">
  Adding a dependency or bumping a version has the classpath resolved again and the index rebuilt behind it, a few seconds
  later, with nothing pressed. It waits for the file to settle first, so a save in the middle of an edit does not start a
  rebuild against half a <code>&lt;dependency&gt;</code>.
</Callout>

<h2>When a sibling module is installed</h2>
<p>
  In a workspace of several projects, <code>mvn install</code> on a module rewrites its jar and pom in your local
  repository — and every project that depends on it is now resolved against a library that no longer exists. Bennu
  notices when the window regains focus (and when the panel opens): the <strong>Dependencies</strong> panel shows a notice
  with a <strong>Reload dependencies</strong> button, and a toast offers the same once per change. The panel itself is
  always drawn from the repository as it is now, so <strong>Resolve tree</strong> and <em>Pulled in transitively</em> agree;
  what stays old until you reload is the editor's classpath — completion, go-to and the library index.
  <strong>Reload dependencies</strong> is also in the toolbar of the panel and in the command palette.
</p>

<h2>Changing what is on disk</h2>
<p>
  The three actions that change the disk — as opposed to reading it again — live in the build tool's window: the Maven
  tool window's actions menu. All three run as background jobs and report in the Jobs panel. So does the work Bennu starts by itself when a project opens: <em>Indexing</em>, <em>Resolving dependencies</em> — a failure there stays in the list, a success clears it — and <em>Analyzing project</em>; and, when you ask for them, <em>Validating project</em> (<strong>Cancel</strong> stops it) and the project-wide vulnerability check, which shows only if it takes more than a couple of seconds.
</p>
<div class="feature-grid">
  <div class="feature-card">
    <div class="fc-eyebrow">When the editor disagrees with the disk</div>
    <div class="fc-title">Re-resolve dependencies &amp; rebuild index</div>
    <div class="fc-desc">
      Drops the cached classpath, reads the local repository again and reindexes — the two halves of "make the editor agree
      with the disk", since either alone leaves the other's stale answer. Rarely needed by hand: saving a pom does it.
    </div>
  </div>
  <div class="feature-card">
    <div class="fc-eyebrow">When the pom is right and the jar is not there</div>
    <div class="fc-title">Download missing dependencies <kbd>Alt</kbd> + <kbd>Shift</kbd> + <kbd>U</kbd></div>
    <div class="fc-desc">
      The one action here that uses the network: <code>dependency:go-offline</code> for the project, then a rebuild.
    </div>
  </div>
  <div class="feature-card">
    <div class="fc-eyebrow">When a library is a decompiled stub</div>
    <div class="fc-title">Download sources</div>
    <div class="fc-desc">
      Fetches every dependency's <code>-sources.jar</code>, so <kbd>Ctrl</kbd> + <kbd>B</kbd> into a library lands on real
      source. Artifacts that publish no sources are skipped, not reported as failures.
    </div>
  </div>
</div>
<p><strong>Editing a pom.xml</strong> covers what the same reading does inside the editor.</p>

<h2>The Dependencies tool window</h2>
<p>
  <kbd>Alt</kbd> + <kbd>N</kbd> shows the classpath as a list, one group per module. Each row carries:
</p>
<ul class="prop-list">
  <li><strong>The coordinate</strong>group and artifact</li>
  <li><strong>The version you actually get</strong>with <code>$&#123;…&#125;</code> expanded and <code>&lt;dependencyManagement&gt;</code> applied</li>
  <li><strong>The scope</strong>and the <code>optional</code> tag where it applies</li>
  <li><strong>Where the answer came from</strong>declared here, pinned by a parent's management, or inherited whole from a parent's own <code>&lt;dependencies&gt;</code></li>
  <li><strong>The profile</strong>when the dependency only exists under a <code>&lt;profile&gt;</code> — shown and labelled, since whether it is active depends on the JDK, the OS and the command line</li>
</ul>
<p>
  Clicking a row opens the pom that decides it — which is usually not the one you were reading.
</p>
<p>
  The last group, <strong>Pulled in transitively</strong>, is every jar on the classpath that no module asked for: where
  "why is <em>this</em> version of that library here" gets answered.
</p>

<h3>Who brings what, and version conflicts</h3>
<p>
  The list above is read from the poms and never runs Maven, so the window opens instantly. Two questions need Maven itself —
  which dependency brought a jar in, and why there is more than one version of it — because Maven's choice between versions
  depends on the whole reactor. The <strong>Resolve tree</strong> button (the flow icon in the toolbar, or the one in the notice
  at the top) runs <code>mvn dependency:tree</code> in the background. It can take a while on a cold cache, so it only runs when
  you ask, honours the <em>automatic download</em> setting (offline otherwise), and keeps its answer per project until you resolve again. While it
  runs it is a row in the <strong>Jobs</strong> panel, like any other long resolve.
</p>
<p>Once it has answered:</p>
<ul class="prop-list">
  <li><strong>Version conflicts</strong>a group at the top with every artifact the project sees at more than one version. Each lists the version Maven kept, the ones it dropped, and for each version the chain of dependencies that asked for it.</li>
  <li><strong>via …</strong>every transitive row names the declared dependencies that bring it in; its chevron opens <strong>Required by</strong>, every route from what a module declared down to it (<code>spring-boot-starter-web → spring-web → spring-core</code>)</li>
  <li><strong>Details</strong>a declared row opens (chevron) to show what it pulls in and, when conflicted, its versions</li>
  <li><strong>Search</strong>the filter also finds a library by what it pulls in — typing <code>freemarker</code> opens the dependencies that bring it, with the match highlighted — and by the routes of a transitive row</li>
  <li><strong>Vulnerability badge</strong>a shield with the count on a dependency with known CVEs — see <strong>Dependency security</strong></li>
</ul>
<p>
  On a Cargo workspace the same button runs <code>cargo tree --workspace</code>. Cargo never drops a version — two incompatible
  versions of a crate are both built — so the group is called <strong>Duplicate versions</strong> and lists them without "kept / dropped".
  Maven runs under the JDK the project is indexed with.
</p>
<p>
  Every group starts <strong>closed</strong>, so opening the panel shows the modules and nothing more; a filter opens the groups it found something in.
  <strong>Show only this module</strong> — on a group header's menu (right-click) or on a module of the selector tree above the list — narrows the panel to that module;
  <strong>Show all</strong> brings the rest back.
</p>
<p>
  Each group header has <strong>Focus in Project</strong> — its locate button, or a right-click on the header — which opens
  the Project tree on that module's (or crate's) folder, expanded and selected, with the keyboard focus on it. The same menu
  opens its <code>pom.xml</code> or <code>Cargo.toml</code> and copies its path.
</p>

<h2>A jar that is not there</h2>
<p>
  A Maven repository often holds a dependency's <code>.pom</code> without its <code>.jar</code>. The pom is what Maven reads
  to walk the graph; the jar arrives only when something compiles against it. So a project whose tree has been resolved but
  never built has the folders and none of the code.
</p>
<p>
  <strong>Allow Maven to use the network</strong>, under Settings → Java, is on by default and closes that gap: when a jar
  is missing, the resolve fetches it. It is the one switch for every place Bennu runs Maven — the resolve, the build before a
  run, the run's classpath and the goals started from the Maven panel, whose <em>Offline</em> toggle flips the same setting.
  Turning it off resolves from the local repository alone — worth doing on a metered connection or behind a slow corporate
  repository.
</p>
<Callout variant="warning" title="What a missing jar looks like">
  A declared dependency whose jar is still missing is called out, because that is exactly what "cannot find symbol" looks
  like in a file that is fine. Until the classpath has been resolved, the panel says the column is unknown rather than
  marking everything missing.
</Callout>

<h2>What it will not compute</h2>
<p>
  Reading the list runs nothing: the poms are files, and the classpath is the one already resolved for the index. The list
  itself does not work out imported BOMs, version ranges or which of two transitive versions wins — a version only they can
  answer stays blank, unless the resolved classpath settles it, which is not a guess but the jar the compiler is being given.
</p>
<p>
  The classpath resolve, by contrast, does apply BOMs, nearest-wins and exclusions, but it never picks a version out of a
  range and never activates a profile — both are decisions about a build. Where those rules come from and what they decide is
  covered in <strong>Dependency versions</strong>.
</p>

<h2>Versions and vulnerabilities</h2>
<p>
  <strong>Dependency versions</strong> explains where a version comes from, scopes, exclusions, duplicates and the
  newer-version offers. <strong>Dependency security</strong> covers the known-vulnerability check and the Vulnerabilities
  window.
</p>

<h2>The module graph</h2>
<p>
  The list answers <em>what does this module need</em>. <strong>Who needs it</strong>, what a change to it rebuilds and
  whether the project has a dependency cycle are properties of the shape instead, and they have their own window —
  <kbd>Alt</kbd> + <kbd>Shift</kbd> + <kbd>D</kbd>, or the network button in the Maven or Cargo tool window's header. See
  <strong>The module graph</strong>.
</p>
