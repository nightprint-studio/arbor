<script lang="ts">
  /**
   * Editing a pom.xml: colour, coordinate completion from the local repository, and the checks. Versions and the
   * network-backed checks live in the Dependency pages.
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
  Nothing is ever dimmed. The tags of each top-level block take that block's own hue, so the file can be scrolled by its
  shape: <code>&lt;dependencies&gt;</code>, <code>&lt;dependencyManagement&gt;</code>, <code>&lt;build&gt;</code>,
  <code>&lt;properties&gt;</code>, <code>&lt;profiles&gt;</code>, the repositories, and the other projects
  (<code>&lt;parent&gt;</code>, <code>&lt;modules&gt;</code>) are each a different colour, and the closing tag matches its opening
  one. The project's own coordinates keep the ordinary tag colour.
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
  </tbody>
</table>
<Callout variant="info" title="Quiet where it cannot be sure">
  Nothing is marked before the repository has been read. A module of your own project is built from source and never looked
  for in a repository. A <code>&lt;dependencyManagement&gt;</code> entry names a version for something this module may not
  even use, so its absence is not a problem. And a plugin, or a dependency that only exists under a <code>&lt;profile&gt;</code>,
  is a warning rather than an error — neither is necessarily fetched on a machine that has not run it.
</Callout>

<h2>Versions, navigation and vulnerabilities</h2>
<p>
  <kbd>Ctrl</kbd> + <kbd>B</kbd> and hover on a coordinate or a <code>$&#123;property&#125;</code>, a dependency's own pom,
  scopes, exclusions and the newer-version offers are covered in <strong>Dependency versions</strong>. The one check that
  sends your coordinates to a third party, the known-vulnerability warning, is covered in <strong>Dependency security</strong>.
</p>

<h2>When something is missing</h2>
<p>
  Nothing on this page downloads anything — that is what makes it instant, and what makes it work with no network.
  <strong>Download dependencies</strong> is the deliberate action that does: it runs Maven's <code>dependency:go-offline</code>
  for the project as a background job, and rebuilds the index when it finishes so library types resolve. It is the fix for a pom
  that is right on a machine that simply does not have the jar yet.
</p>
