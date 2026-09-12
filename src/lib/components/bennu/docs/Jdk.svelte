<script lang="ts">
  /**
   * The JDK: the language level a project is read at, where that comes from, which installation is used, and what
   * goes wrong when either is off.
   */
  import Callout from '$lib/components/shared/ui/Callout.svelte';
</script>

<span class="eyebrow">Projects</span>
<h1>The JDK</h1>

<p class="doc-lead">
  Which Java a project is read with, where that answer comes from, and what changes when it is wrong.
</p>

<h2>The language level</h2>
<p>
  The footer shows the Java language level Bennu resolved and where it came from. When it cannot be inferred the footer
  reads <code>JDK —</code>. The places it is read from:
</p>
<ul class="prop-list">
  <li><code>maven.compiler.source</code>the usual one</li>
  <li><code>maven.compiler.release</code>the newer spelling of the same thing</li>
  <li><code>&lt;java.version&gt;</code>the Spring Boot property</li>
  <li><strong>The compiler plugin</strong>its own <code>&lt;configuration&gt;</code></li>
  <li><strong>Toolchains</strong>when the build uses them</li>
  <li><strong>A manual override</strong>when you have set one</li>
</ul>

<h2>A multi-module project</h2>
<p>
  The whole reactor is read, not only its root. An aggregator pom usually declares no level at all — it exists to list
  <code>&lt;modules&gt;</code> — so the highest level any module declares becomes the project's.
</p>
<Callout variant="info" title="Why the highest">
  The index has one language level, and the choice is not symmetric: too low invents errors in the module that
  legitimately uses newer syntax, while too high can only stay silent about an older one.
</Callout>
<p>
  And the level in force <strong>follows the module you are in</strong>. A reactor part-way through a migration has one
  module on 21 and another still on 8, which is an ordinary state: the validator's version checks and the postfix
  templates ask the open file's own module, and the footer names it beside the level.
</p>
<p>
  Only that question is answered per module. The index and the dependency classpath are one JDK by construction, since
  an index holds one standard library.
</p>
<p>
  The project tree says the same on every module and crate row — <code>JDK 21</code>, <code>JDK 21 · war</code>,
  <code>Rust 2024 · bin</code> — and its tooltip carries the artifact id, the packaging and the key that declared the
  level.
</p>

<h2>The installation</h2>
<p>
  The standard library is resolved against an installed JDK. Bennu looks for one in this order:
</p>
<ol class="step-list">
  <li>The extra JDK directories in <strong>Settings → Java → JDK locations</strong>.</li>
  <li><code>JAVA_HOME</code>.</li>
  <li>Each platform's usual places: the <code>JavaVirtualMachines</code> bundles on macOS, the Program Files vendor
    directories on Windows, <code>/usr/lib/jvm</code> on Linux, the Homebrew <code>openjdk</code> formula, and the
    directories a version manager or an IDE installs JDKs into.</li>
</ol>
<p>
  The one whose level matches the project wins; failing that, the newest installed.
</p>
<Callout variant="warning" title="No JDK, nothing resolves">
  When none is found the title bar carries a <strong>No JDK</strong> warning, because without one nothing — not even
  <code>String</code> — resolves.
</Callout>

<h2>Builds and tests use the same one</h2>
<p>
  The installation Bennu found is handed to Maven as <code>JAVA_HOME</code>, so the level your code is analysed at is
  the level it is compiled at.
</p>
<p>
  If no JDK of that level is installed, the build inherits whatever <code>JAVA_HOME</code> your environment already sets
  instead of being pointed at a different one.
</p>
<Callout variant="info" title="Why not a different one">
  A compiler of the wrong version fails with a message about the target release, which says nothing about the JDK that
  caused it.
</Callout>
