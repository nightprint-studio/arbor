<!-- Bennu docs — dependencies: the tool window, sources, and what is reachable inside them. -->
<h1>Dependencies</h1>
<p class="doc-lead">
  The libraries a project pulls in — listed, searchable, and (with sources) readable in the same
  editor as your own code.
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
  A declared dependency with no jar in your local repository is called out, because that is exactly
  what "cannot find symbol" looks like in a file that is fine. Until the classpath has been resolved
  the panel says the column is unknown instead of marking everything missing. The last group,
  <strong>Pulled in transitively</strong>, is every jar on the classpath that no module asked for —
  where "why is <em>this</em> version of that library here" gets answered.
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
