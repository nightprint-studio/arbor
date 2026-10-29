<script lang="ts">
  /**
   * Dependency versions: where a version comes from, a dependency's own pom, scopes, exclusions, duplicates and
   * conflicts, and the newer-version offers (the one check here that reaches the network).
   */
  import Callout from '$lib/components/shared/ui/Callout.svelte';
</script>

<span class="eyebrow">Projects</span>
<h1>Dependency versions</h1>

<p class="doc-lead">
  A dependency line rarely says which version you get. It is written in a property, inherited from a parent, pinned by a
  bill of materials or decided by a conflict three levels down. Bennu follows each of those and says where the answer
  lives.
</p>

<h2>Where a version comes from</h2>
<p>
  The version a pom resolves to is found the way Maven finds it, nearest first:
</p>
<ol class="step-list">
  <li>A <code>&lt;version&gt;</code> written on the dependency itself.</li>
  <li>A <code>$&#123;property&#125;</code> in it, expanded against the properties of the pom that <em>wrote</em> the string, then
    those of its parents.</li>
  <li>With no <code>&lt;version&gt;</code> at all, the <code>&lt;dependencyManagement&gt;</code> of this pom, then of each parent up the chain.</li>
  <li>A BOM imported there (<code>&lt;scope&gt;import&lt;/scope&gt;</code>, <code>&lt;type&gt;pom&lt;/type&gt;</code>) sits <em>behind</em> the pom
    that imports it: an explicit entry beside the import wins, which is how a project pins one library away from what the
    BOM says.</li>
</ol>
<p>
  Reading a pom means opening several files, and every one of them is a jump from here:
</p>
<table>
  <thead><tr><th><kbd>Ctrl</kbd> + <kbd>B</kbd> on</th><th>Goes to</th></tr></thead>
  <tbody>
    <tr><td>A coordinate of one of your modules</td><td>The module that builds it</td></tr>
    <tr><td>A coordinate something else pins</td><td>The parent's <code>&lt;dependencyManagement&gt;</code> entry, or the BOM's own <code>.pom</code> when a BOM supplies it</td></tr>
    <tr><td>Any other coordinate</td><td>The artifact's own <code>.pom</code> in the repository — where what it drags in is written</td></tr>
    <tr><td>A <code>$&#123;property&#125;</code></td><td>The pom that defines it, on the line that does</td></tr>
    <tr><td>A <code>&lt;module&gt;</code></td><td>That module</td></tr>
  </tbody>
</table>
<p>
  Hover says the same without moving: what the coordinate resolves to on disk, what the version expands to and who decided it,
  the scope, and every version of that artifact you have installed. Completing a <code>&lt;version&gt;</code> offers the
  managed one first, naming the pom that decided it.
</p>
<Callout variant="info" title="What it leaves unresolved">
  A version range (<code>[1.0,2.0)</code>) is never turned into a version — picking one is a decision about a build, and
  inventing it would produce a classpath no build has. A declaration whose version only a range or an undefined property
  can answer is left off the classpath rather than guessed.
</Callout>

<h3>Checked on the way</h3>
<table>
  <thead><tr><th>Marked</th><th>What it means</th></tr></thead>
  <tbody>
    <tr><td>Parent not found</td><td>The <code>&lt;parent&gt;</code> is neither beside the pom (through <code>&lt;relativePath&gt;</code>) nor in the local repository. An error: every managed version and inherited dependency it should have supplied is missing, which otherwise surfaces as many unrelated "no version" reports. A reactor's own parent is built from source and does not need to be installed.</td></tr>
    <tr><td>Moved to other coordinates</td><td>The old coordinates publish a jarless pom that only redirects. Maven follows it, so the build works; the warning names the new coordinates, and updating removes the indirection.</td></tr>
  </tbody>
</table>
<p>
  A missing version, a version already managed, an undefined property, a misspelled scope and the repository checks
  (artifact or version not installed) are described in <strong>Editing a pom.xml</strong>.
</p>

<h2>A dependency's own pom</h2>
<p>
  The <code>.pom</code> you land in from a coordinate lives in the local repository, not in your project — and it is still a
  pom. The colour, the hover and the same <kbd>Ctrl</kbd> + <kbd>B</kbd> on <em>its</em> dependencies and its
  <code>&lt;parent&gt;</code> all work there, so "what does this actually drag in" is followed as far as it goes, not one step.
</p>
<Callout variant="info" title="A pom in the repository is never marked">
  The checks are written against a file you can fix. An artifact was built against dependencies your machine had no reason to
  download, so there the check that finds a real problem in your own pom would underline the ordinary state of a repository,
  on a line nobody can edit.
</Callout>

<h2>Scopes</h2>
<p>
  A <code>&lt;scope&gt;</code> that is not <code>compile</code>, <code>provided</code>, <code>runtime</code>, <code>test</code>,
  <code>system</code> or <code>import</code> is marked as a warning: Maven treats it as <code>compile</code> without a word.
  Completion offers the valid ones with what each means, and the Dependencies tool window shows every row's scope as a
  coloured badge, since <code>test</code> changes what a dependency is for.
</p>
<p>
  The classpath Bennu resolves for the index applies Maven's own rules to what a dependency drags in:
</p>
<table>
  <thead><tr><th>Declared as</th><th>Reached through</th><th>Ends up</th></tr></thead>
  <tbody>
    <tr><td><code>compile</code></td><td><code>compile</code></td><td><code>compile</code></td></tr>
    <tr><td><code>runtime</code></td><td><code>compile</code></td><td><code>runtime</code></td></tr>
    <tr><td>anything</td><td><code>provided</code></td><td><code>provided</code></td></tr>
    <tr><td>anything</td><td><code>test</code></td><td><code>test</code></td></tr>
    <tr><td>anything</td><td><code>runtime</code></td><td><code>runtime</code></td></tr>
    <tr><td><code>test</code>, <code>provided</code>, <code>system</code></td><td>a dependency of a dependency</td><td>not inherited</td></tr>
    <tr><td><code>&lt;optional&gt;true&lt;/optional&gt;</code></td><td>a dependency of a dependency</td><td>stops at the artifact that declares it</td></tr>
  </tbody>
</table>
<p>
  A <code>system</code>-scoped dependency lives at its <code>&lt;systemPath&gt;</code>, so it is never looked for in the
  repository and never reported missing.
</p>

<h2>Exclusions</h2>
<p>
  <code>&lt;exclusions&gt;</code> are honoured when the classpath is resolved: an excluded artifact is held out of the whole
  subtree beneath the dependency that lists it, and the wildcard form
  (<code>&lt;groupId&gt;*&lt;/groupId&gt;&lt;artifactId&gt;*&lt;/artifactId&gt;</code>) cuts the subtree entirely. An
  <code>&lt;exclusion&gt;</code> names something that is by definition not on the classpath, so it is never marked as missing.
</p>
<Callout variant="tip" title="Not surfaced elsewhere">
  There is no dedicated exclusion check, hover or action, and the Dependencies tool window does not list a dependency's
  exclusions. To see what an exclusion changed, compare against <strong>Resolve tree</strong>.
</Callout>

<h2>Duplicates and conflicts</h2>
<ul class="prop-list">
  <li><strong>Declared twice</strong>the same artifact twice in one <code>&lt;dependencies&gt;</code> block (or one
    <code>&lt;dependencyManagement&gt;</code>) is a warning on the second, because Maven keeps the last silently. The same artifact in management
    and in the dependencies, or in two profiles, is the normal way to write a pom and is left alone.</li>
  <li><strong>Two versions on the classpath</strong>the nearest declaration wins: the version the project declares beats the one a
    dependency asks for, and a later, deeper sighting of the same artifact contributes nothing.</li>
  <li><strong>Management overrides transitives</strong>a version pinned in the reactor's <code>&lt;dependencyManagement&gt;</code>
    (including an imported BOM) applies to the whole graph, not only to what the project declares — so the BOM decides which Jackson you
    get, whoever asked for it. Where two modules pin the same artifact differently, the first module's is used, as one classpath serves the project.</li>
</ul>
<p>
  The classpath is one answer for the whole project, so it cannot show <em>why</em> a version won. The <strong>Resolve
  tree</strong> button in the Dependencies tool window runs <code>mvn dependency:tree</code> and lists every artifact seen at more than
  one version — the one Maven kept, the ones it dropped, and the chain that asked for each. See <strong>Dependencies</strong>.
</p>
<Callout variant="info" title="Profiles are read, not activated">
  A dependency written under a <code>&lt;profile&gt;</code> in your own poms is used when its jar is in the repository, and its
  absence is never reported — whether a profile is on is a fact about a build. In a library's pom, a profile is ignored: no
  downstream build can switch it on.
</Callout>

<h2>When a newer version exists</h2>
<p>
  Two different things say a dependency is behind, and they answer different questions.
</p>
<div class="feature-grid two-col">
  <div class="feature-card">
    <div class="fc-eyebrow">Hint, local</div>
    <div class="fc-title">A newer version, already here</div>
    <div class="fc-desc">
      A hint on the version when a newer release is <em>already in your local repository</em>, so switching costs nothing.
      Only the same numbering scheme counts (<code>2025.1</code> is not newer than <code>6.1.3</code>), and a pre-release is never
      suggested over a release.
    </div>
  </div>
  <div class="feature-card">
    <div class="fc-eyebrow">Offer, network</div>
    <div class="fc-title">A line above the dependency</div>
    <div class="fc-desc">
      The version Maven Central has, in the accent colour above the dependency; pressing it writes that version in place.
    </div>
  </div>
</div>
<Callout variant="warning" title="Reaches the network">
  The local repository holds only what somebody here has already asked for, so by its measure a dependency nobody updated is
  permanently current. Turn the Central check off in <strong>Settings → Java → Check Maven Central for newer versions</strong>;
  the Java side is then entirely local again.
</Callout>
<ul>
  <li>Only what <em>this</em> pom pins. A version inherited from a parent or a BOM, or written as a <code>$&#123;property&#125;</code>,
    is left alone: the line the hint would sit above is not the line that would have to change. Entries in
    <code>&lt;dependencyManagement&gt;</code> get none either — the parent's own offers appear when the parent is open.</li>
  <li>Answers are cached on disk for a day per artifact, and a failed lookup falls back to whatever is cached, however old — on a
    train, last week's answer is the right one.</li>
  <li>A milestone or a release candidate is never offered as the newer version.</li>
  <li>One pass fetches a bounded number of artifacts; a pom with many dependencies and a cold cache fills in over a few passes
    while typing never waits.</li>
  <li>One dependency is updated at a time: there is no update-all, and no action that edits a parent or a BOM.</li>
</ul>
<p>
  Whether the version you have is <em>safe</em> is a separate question, answered in <strong>Dependency security</strong>.
</p>
