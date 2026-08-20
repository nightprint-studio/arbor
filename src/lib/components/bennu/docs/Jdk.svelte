<!-- Bennu docs — the JDK: which one a project uses and how it is chosen. -->
<h1>The JDK</h1>
<p class="doc-lead">
  Which Java a project is read with, where that comes from, and what changes when it is wrong.
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
<p>
  <strong>Builds and test runs use that same install</strong>: it is handed to Maven as
  <code>JAVA_HOME</code>, so the level your code is analysed at is the level it is compiled at. If no
  JDK of that level is installed, the build inherits whatever <code>JAVA_HOME</code> your environment
  already sets rather than being pointed at a different one — a compiler of the wrong version fails
  with a message about the target release, which says nothing about the JDK that caused it.
</p>
