<script lang="ts">
  /**
   * Dependencies: how the classpath is resolved without a build, the actions that change what is on disk, and the
   * tool window that lists it.
   */
  import Callout from '$lib/components/shared/ui/Callout.svelte';
</script>

<span class="eyebrow">Projects</span>
<h1>Dependencies</h1>

<p class="doc-lead">
  The libraries a project pulls in — listed, searchable and, with sources, readable in the same editor as your own code.
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
  <li>The result is cached against your poms' timestamps <em>and</em> against what was missing.</li>
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

<h2>Changing what is on disk</h2>
<p>
  The three actions that change the disk — as opposed to reading it again — live in the build tool's window: the Maven
  tool window's actions menu. All three run as background jobs and report in the Jobs panel.
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
  <strong>Download missing dependencies</strong>, under Settings → Java, is on by default and closes that gap: when a jar
  is missing, the resolve fetches it. Turning it off resolves from the local repository alone — worth doing on a metered
  connection or behind a slow corporate repository.
</p>
<Callout variant="warning" title="What a missing jar looks like">
  A declared dependency whose jar is still missing is called out, because that is exactly what "cannot find symbol" looks
  like in a file that is fine. Until the classpath has been resolved, the panel says the column is unknown rather than
  marking everything missing.
</Callout>

<h2>What it will not compute</h2>
<p>
  Reading the list runs nothing: the poms are files, and the classpath is the one already resolved for the index. Imported
  BOMs and version ranges are the two things it will not work out — a version only they can answer stays blank, unless the
  resolved classpath settles it, which is not a guess but the jar the compiler is being given.
</p>

<h2>The module graph</h2>
<p>
  The list answers <em>what does this module need</em>. <strong>Who needs it</strong>, what a change to it rebuilds and
  whether the project has a dependency cycle are properties of the shape instead, and they have their own window —
  <kbd>Alt</kbd> + <kbd>Shift</kbd> + <kbd>D</kbd>, or the network button in the Maven or Cargo tool window's header. See
  <strong>The module graph</strong>.
</p>
