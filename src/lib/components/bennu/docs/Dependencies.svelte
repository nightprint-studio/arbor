<!-- Bennu docs — dependencies: the tool window, sources, and what is reachable inside them. -->
<h1>Dependencies</h1>
<p class="doc-lead">
  The libraries a project pulls in — listed, searchable, and (with sources) readable in the same
  editor as your own code.
</p>

<h2>Dependencies</h2>
<p>
  For a Maven project Bennu resolves the dependency jars from your local repository so completion,
  navigation and validation see library types, not just the JDK and your own sources. Every module
  of a multi-module project contributes its own dependencies. The resolve is
  <strong>offline</strong> — nothing is downloaded — so a dependency that has never been fetched
  cannot be resolved, and Bennu names it rather than reporting a number.
</p>
<p>
  It is read straight from the poms and your local repository: the parent chains, the imported BOMs,
  the transitive closure and the exclusions, in milliseconds and with no build tool involved. Maven
  is only run when that reading comes up short — and then the two are combined, because a
  <code>mvn</code> run that fails halfway still wrote the entries it did resolve. A project whose
  Maven is missing, mis-configured or simply broken still gets a classpath.
</p>
<p>
  The result is cached against your poms' timestamps <em>and</em> against what was missing: the
  moment one of the absent artifacts lands in your repository the cache is stale, so installing a
  dependency is picked up on the next open instead of being shadowed until a pom is edited.
  <strong>Rebuild index</strong> re-resolves unconditionally. Maven, when it is needed, is looked
  for on <code>PATH</code>, then in the usual install directories, then as the project's own
  <code>mvnw</code> wrapper.
</p>
<p>
  The panel's header carries the three things that change what is <em>on disk</em>, as opposed to
  the refresh beside them which only re-reads it. All three are background jobs and report in the
  Jobs panel.
</p>
<div class="fc-list">
  <div class="fc-item">
    <div class="fc-title">Re-resolve dependencies &amp; rebuild index</div>
    <div class="fc-desc">
      Drops the cached classpath, re-reads the local repository, and reindexes. The two halves of
      "make the editor agree with what is on disk" — doing either alone leaves you with the other's
      stale answer.
    </div>
  </div>
  <div class="fc-item">
    <div class="fc-title">Download missing dependencies <kbd>Alt</kbd> + <kbd>Shift</kbd> + <kbd>U</kbd></div>
    <div class="fc-desc">
      The one action here that uses the network: <code>dependency:go-offline</code> for the project,
      then a rebuild. The fix for the state where the pom is right and the machine just does not
      have the jar yet.
    </div>
  </div>
  <div class="fc-item">
    <div class="fc-title">Download sources</div>
    <div class="fc-desc">
      Fetches the <code>-sources.jar</code> of every dependency, so <kbd>Ctrl</kbd> + <kbd>B</kbd>
      into a library lands on real source instead of a decompiled stub. Artifacts that publish no
      sources are skipped rather than reported as failures.
    </div>
  </div>
</div>
<p>
  See <strong>Editing a pom.xml</strong> for what the same reading does inside the editor.
</p>
<p>
  <strong>The Dependencies tool window</strong> (<kbd>Alt</kbd> + <kbd>N</kbd>) shows that as a list,
  one group per module. Each row carries the coordinate, the version <em>you actually get</em> — with
  <code>$&#123;…&#125;</code> expanded and <code>&lt;dependencyManagement&gt;</code> applied — the scope, and
  where the answer came from: declared here, pinned by a parent's management, or inherited whole
  from a parent's own <code>&lt;dependencies&gt;</code>. Clicking a row opens the pom that decides
  it, which is usually not the one you were reading. Rows are also tagged
  <code>optional</code>, and a dependency that only exists under a <code>&lt;profile&gt;</code> says
  which one — whether that profile is active depends on the JDK, the OS and the command line, so it
  is shown and labelled rather than guessed at.
</p>
<p>
  When a dependency's jar is not in the local repository yet, the resolve fetches it. A Maven
  repository often holds a dependency's <code>.pom</code> without its <code>.jar</code> — the pom is
  what Maven reads to walk the dependency graph, and the jar arrives only when something compiles
  against it — so a project whose tree has been resolved but never built has the folders and none of
  the code. <strong>Download missing dependencies</strong>, under Settings → Java, is on by default
  and is what closes that gap; turning it off resolves from the local repository alone, which is
  worth doing on a metered connection or behind a slow corporate repository.
</p>
<p>
  A declared dependency whose jar is still missing after that is called out, because that is exactly
  what "cannot find symbol" looks like in a file that is fine. Until the classpath has been resolved
  the panel says the column is unknown instead of marking everything missing. The last group,
  <strong>Pulled in transitively</strong>, is every jar on the classpath that no module asked for —
  where "why is <em>this</em> version of that library here" gets answered.
</p>
<p>
  Each group header carries <strong>Focus in Project</strong> — its locate button, or a right-click
  on the header — which opens the Project tree on that module's (or crate's) folder, expanded and
  selected, with the keyboard focus on it. The same menu opens its <code>pom.xml</code> /
  <code>Cargo.toml</code> and copies its path.
</p>
<p>
  Reading it runs nothing: the poms are files, and the classpath is the one already resolved for the
  index. Imported BOMs and version ranges are the two things it will not compute — a version only
  they can answer stays blank unless the resolved classpath settles it, which is not a guess but the
  jar the compiler is being handed.
</p>

<h3>The module graph</h3>
<p>
  The list answers <em>what does this module need</em>. <strong>Who needs it</strong>, what a change to
  it rebuilds, and whether the project has a dependency cycle are properties of the shape instead, and
  they live in their own window — <kbd>Alt</kbd> + <kbd>Shift</kbd> + <kbd>D</kbd>, or the network
  button in this panel's header. See <strong>The module graph</strong> for what it draws and what each
  line means.
</p>
