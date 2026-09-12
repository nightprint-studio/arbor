<script lang="ts">
  /**
   * Editing a pom.xml: colour, coordinate completion from the local repository, the checks, where a version comes from,
   * and the one thing that reaches the network.
   */
  import Callout from '$lib/components/shared/ui/Callout.svelte';
  import { highlightCode } from '$lib/utils/highlight';
</script>

<span class="eyebrow">Projects</span>
<h1>Editing a pom.xml</h1>

<p class="doc-lead">
  A pom is the one file in a Java project whose correctness is a fact about your machine. Bennu reads your local
  repository and answers from it — completing coordinates that are actually there, and marking the ones that are not.
</p>

<h2>Colour</h2>
<p>
  An XML mode has one colour for tag names and none for the text between them, and a pom is almost nothing but text
  between tags. Here it is coloured by what each thing <em>is</em>:
</p>
<ul class="prop-list">
  <li><strong>Names</strong>an <code>artifactId</code>, a module</li>
  <li><strong>Literals</strong>a version</li>
  <li><strong>Fixed words</strong>a scope, a packaging, a lifecycle phase</li>
  <li><strong>Properties</strong>one colour where a property is declared and where it is used</li>
</ul>
<p>
  Nothing is ever dimmed. Every tag keeps the colour it has elsewhere; sections and the blocks inside them get
  <em>weight</em> in that same colour, so the file can be scrolled by its shape without any of it becoming harder to read.
</p>
<pre><code>{@html highlightCode(`<properties>
    <spring.version>5.3.39</spring.version>
</properties>
…
<dependency>
    <groupId>org.springframework</groupId>
    <artifactId>spring-context</artifactId>
    <version>\${spring.version}</version>
</dependency>`, 'markup')}</code></pre>
<Callout variant="info" title="A property is not coloured as its value">
  <code>$&#123;spring.version&#125;</code> is tinted as a substitution, its name in the colour a property has everywhere
  else. The only mistake possible there is the two ends not matching, and one colour is what makes that visible.
</Callout>

<h2>The local repository</h2>
<p>
  Every answer on this page comes from the repository your build would use — not <code>~/.m2/repository</code> assumed, but
  found the way Maven finds it:
</p>
<ol class="step-list">
  <li><code>-Dmaven.repo.local</code> in <code>MAVEN_OPTS</code>.</li>
  <li><code>&lt;localRepository&gt;</code> in your <code>settings.xml</code>.</li>
  <li>The default.</li>
</ol>
<Callout variant="tip" title="Nothing resolves, yet the terminal builds">
  <strong>Dependencies</strong> reports which repository is in use — the first thing to check on a machine that builds fine
  from a terminal.
</Callout>
<p>
  It is walked once into a list of coordinates and cached, so completion is instant. The walk runs in the background the first
  time a Maven project is opened; until it lands, completion is thinner and <em>nothing is marked as missing</em> — an empty
  answer means "not read yet", never "you do not have it".
</p>

<h2>Completion</h2>
<div class="feature-grid two-col">
  <div class="feature-card">
    <div class="fc-eyebrow">Coordinates</div>
    <div class="fc-title"><code>&lt;groupId&gt;</code> and <code>&lt;artifactId&gt;</code></div>
    <div class="fc-desc">
      Every coordinate in your repository, each with its newest installed version, then a built-in table of the libraries a Java
      project usually reaches for, marked <em>not installed</em>. Completing an artifactId while the groupId above it is empty
      <strong>fills both</strong> in one edit.
    </div>
  </div>
  <div class="feature-card">
    <div class="fc-eyebrow">Versions</div>
    <div class="fc-title"><code>&lt;version&gt;</code></div>
    <div class="fc-desc">
      The versions you have, newest first. One a <code>&lt;dependencyManagement&gt;</code> entry supplies comes first and says
      which pom decided it; a <code>$&#123;…&#125;</code> property already holding a version of that library is offered too —
      usually what the line should have said.
    </div>
  </div>
  <div class="feature-card">
    <div class="fc-eyebrow">Anywhere a <code>$&#123;</code> is typed</div>
    <div class="fc-title"><code>$&#123;properties&#125;</code></div>
    <div class="fc-desc">
      Every property in scope — this pom's, its parents', the implicit <code>project.*</code> ones — each with the value it expands
      to and, when decided elsewhere, the pom that decided it. This pom's first, then the inherited, then the implicit ones.
    </div>
  </div>
  <div class="feature-card">
    <div class="fc-eyebrow">Fixed vocabularies</div>
    <div class="fc-title">Scopes, types, phases, modules</div>
    <div class="fc-desc">
      <code>&lt;scope&gt;</code>, <code>&lt;type&gt;</code>, <code>&lt;packaging&gt;</code>, <code>&lt;optional&gt;</code> and an
      execution's <code>&lt;phase&gt;</code>, each with what it means. <code>&lt;module&gt;</code> completes the directories beside
      the pom that hold one and are not listed yet.
    </div>
  </div>
</div>

<h2>What gets marked</h2>
<p>
  A dependency whose jar is not in the repository is underlined <strong>where it is written</strong>. That is the point of the
  whole feature: an artifact that was never downloaded makes every type in it unresolvable at once, in files that are perfectly
  correct — and without this, the pom says nothing about it.
</p>
<table>
  <thead><tr><th>Marked</th><th>What it means</th></tr></thead>
  <tbody>
    <tr><td>Not in the local repository</td><td>Nothing has ever downloaded this coordinate. When a near-identical artifactId <em>is</em> installed, it is suggested.</td></tr>
    <tr><td>Version not installed</td><td>The artifact is there at other versions — which are listed, so a mistyped version is obvious.</td></tr>
    <tr><td>Undefined <code>$&#123;property&#125;</code></td><td>Nothing in this pom or its parents defines it. In a coordinate that is an error — Maven resolves it to the literal text and then finds no artifact by that name. Elsewhere it is a warning, because a plugin may supply the value during the build; inside a <code>&lt;configuration&gt;</code>, where those live, nothing is reported.</td></tr>
    <tr><td>No version</td><td>Declared with no version, and nothing in management supplies one.</td></tr>
    <tr><td>Version already managed</td><td>Identical to what a parent's <code>&lt;dependencyManagement&gt;</code> says — harmless, but it will not follow when the parent moves.</td></tr>
    <tr><td>Declared twice</td><td>The same artifact twice in one block. Maven keeps the last, silently.</td></tr>
    <tr><td>Not a Maven scope</td><td>A misspelled <code>&lt;scope&gt;</code> is treated as <code>compile</code> without a word.</td></tr>
    <tr><td>Missing module</td><td>A <code>&lt;module&gt;</code> with no <code>pom.xml</code> — the reactor drops it, and every type in it stops resolving everywhere else.</td></tr>
    <tr><td>A newer version, already here</td><td>A hint, not a problem: a newer release of the library is already in your repository, so switching costs nothing.</td></tr>
  </tbody>
</table>
<Callout variant="info" title="Quiet where it cannot be sure">
  Nothing is marked before the repository has been read. A module of your own project is built from source and never looked
  for in a repository. A <code>&lt;dependencyManagement&gt;</code> entry names a version for something this module may not
  even use, so its absence is not a problem. And a plugin, or a dependency that only exists under a <code>&lt;profile&gt;</code>,
  is a warning rather than an error — neither is necessarily fetched on a machine that has not run it.
</Callout>

<h2>Where a version comes from</h2>
<p>
  Reading a pom means opening four files, and every one of them is a jump from here:
</p>
<table>
  <thead><tr><th><kbd>Ctrl</kbd> + <kbd>B</kbd> on</th><th>Goes to</th></tr></thead>
  <tbody>
    <tr><td>A coordinate of one of your modules</td><td>The module that builds it</td></tr>
    <tr><td>A coordinate something else pins</td><td>The parent's <code>&lt;dependencyManagement&gt;</code> entry</td></tr>
    <tr><td>Any other coordinate</td><td>The artifact's own <code>.pom</code> in the repository — where what it drags in is written</td></tr>
    <tr><td>A <code>$&#123;property&#125;</code></td><td>The pom that defines it, on the line that does</td></tr>
    <tr><td>A <code>&lt;module&gt;</code></td><td>That module</td></tr>
  </tbody>
</table>
<p>
  Hover says the same without moving: what the coordinate resolves to on disk, what the version expands to and who decided it,
  the scope, and every version of that artifact you have installed.
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

<h2>When a newer version exists</h2>
<p>
  Above a dependency that is behind, a line says which version Maven Central has, and one press writes it.
</p>
<Callout variant="warning" title="The one thing here that reaches the network">
  The local repository holds only what somebody here has already asked for, so by its measure a dependency nobody updated is
  permanently current. Turn the check off in <strong>Settings → Java → Check Maven Central for newer versions</strong>.
</Callout>
<ul>
  <li>Only what <em>this</em> pom pins. A version inherited from a parent or a BOM, or written as a <code>$&#123;property&#125;</code>,
    is left alone: the line the hint would sit above is not the line that would have to change.</li>
  <li>Answers are cached on disk for a day per artifact, and a failed lookup falls back to whatever is cached, however old — on a
    train, last week's answer is the right one.</li>
  <li>A milestone or a release candidate is never offered as the newer version.</li>
</ul>

<h2>When something is missing</h2>
<p>
  Nothing on this page downloads anything — that is what makes it instant, and what makes it work with no network.
  <strong>Download dependencies</strong> is the deliberate action that does: it runs Maven's <code>dependency:go-offline</code>
  for the project as a background job, and rebuilds the index when it finishes so library types resolve. It is the fix for a pom
  that is right on a machine that simply does not have the jar yet.
</p>
