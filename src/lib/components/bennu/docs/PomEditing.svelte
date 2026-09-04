<!-- Bennu docs — the pom.xml editor: coordinate completion, the checks, and where a version comes from. -->
<h1>Editing a pom.xml</h1>
<p class="doc-lead">
  A pom is the one file in a Java project whose correctness is a fact about your machine. Bennu
  reads your local repository and answers from it — completing coordinates that are actually there,
  and marking the ones that are not.
</p>

<h2>The local repository</h2>
<p>
  Every answer on this page comes from the repository your build would use — not from
  <code>~/.m2/repository</code> assumed, but resolved the way Maven resolves it:
  <code>-Dmaven.repo.local</code> in <code>MAVEN_OPTS</code> first, then
  <code>&lt;localRepository&gt;</code> in your <code>settings.xml</code>, then the default.
  <strong>Dependencies</strong> reports which one is in use, and it is the first thing to check when
  nothing resolves on a machine that builds fine from a terminal.
</p>
<p>
  It is walked once into a list of coordinates and cached, so completion is instant. The walk runs
  in the background the first time a Maven project is opened: until it lands, completion is thinner
  and <em>nothing is marked as missing</em> — an empty answer means "not read yet", never "you do
  not have it".
</p>

<h2>Completion</h2>
<div class="fc-list">
  <div class="fc-item">
    <div class="fc-title"><code>&lt;groupId&gt;</code> and <code>&lt;artifactId&gt;</code></div>
    <div class="fc-desc">
      Every coordinate in your repository, each showing its newest installed version, followed by a
      built-in table of the libraries a Java project usually reaches for — marked
      <em>not installed</em>, because those need a download. Completing an artifactId while the
      groupId above it is still empty <strong>fills both</strong> in one edit.
    </div>
  </div>
  <div class="fc-item">
    <div class="fc-title"><code>&lt;version&gt;</code></div>
    <div class="fc-desc">
      The versions you have, newest first. When a <code>&lt;dependencyManagement&gt;</code> entry
      already supplies one it is offered first and says which pom decided it. A
      <code>$&#123;…&#125;</code> property that already holds a version for that library is offered
      too — usually what the line should have said.
    </div>
  </div>
  <div class="fc-item">
    <div class="fc-title"><code>$&#123;properties&#125;</code></div>
    <div class="fc-desc">
      Every property in scope — this pom's, its parents', and the implicit
      <code>project.*</code> ones — anywhere a <code>$&#123;</code> is typed, in any element. Each
      shows the value it expands to and, when it was decided somewhere else, the pom that decided
      it. This pom's own come first, then what it inherits, then the implicit ones nobody is
      looking for.
    </div>
  </div>
  <div class="fc-item">
    <div class="fc-title">Fixed vocabularies</div>
    <div class="fc-desc">
      <code>&lt;scope&gt;</code>, <code>&lt;type&gt;</code>, <code>&lt;packaging&gt;</code>,
      <code>&lt;optional&gt;</code> and a plugin execution's <code>&lt;phase&gt;</code>, each with
      what it means. <code>&lt;module&gt;</code> completes the directories beside the pom that hold
      one and are not listed yet.
    </div>
  </div>
</div>

<h2>What gets marked</h2>
<p>
  A dependency whose jar is not in the repository is underlined <strong>where it is written</strong>.
  That is the point of the whole feature: an artifact that was never downloaded makes every type in
  it unresolvable at once, in files that are perfectly correct, and without this the pom says
  nothing about it.
</p>
<table class="doc-table">
  <thead><tr><th>Marked</th><th>What it means</th></tr></thead>
  <tbody>
    <tr><td>Not in the local repository</td><td>Nothing has ever downloaded this coordinate. When a near-identical artifactId <em>is</em> installed, it is suggested.</td></tr>
    <tr><td>Version not installed</td><td>The artifact is there at other versions — which are listed, so a mistyped version is obvious.</td></tr>
    <tr><td>Undefined <code>$&#123;property&#125;</code></td><td>Nothing in this pom or its parents defines it. In a coordinate that is an error — Maven resolves it to the literal text and then fails to find an artifact by that name. Anywhere else in the pom it is a warning, because a plugin may supply the value during the build; inside a <code>&lt;configuration&gt;</code>, where those live, nothing is reported at all.</td></tr>
    <tr><td>No version</td><td>Declared with no version and nothing in management supplies one.</td></tr>
    <tr><td>Version already managed</td><td>Identical to what a parent's <code>&lt;dependencyManagement&gt;</code> says — harmless, but it will not follow when the parent moves.</td></tr>
    <tr><td>Declared twice</td><td>The same artifact twice in one block. Maven keeps the last, silently.</td></tr>
    <tr><td>Not a Maven scope</td><td>A misspelled <code>&lt;scope&gt;</code> is treated as <code>compile</code> without a word.</td></tr>
    <tr><td>Missing module</td><td>A <code>&lt;module&gt;</code> with no <code>pom.xml</code> — the reactor drops it and every type in it stops resolving everywhere else.</td></tr>
    <tr><td>A newer version, already here</td><td>A hint, not a problem: a newer release of that library is already in your repository, so switching costs nothing.</td></tr>
  </tbody>
</table>
<p>
  The checks stay quiet where they cannot be sure. Nothing is marked before the repository has been
  read; a module of your own project is built from source and is never looked for in a repository; a
  <code>&lt;dependencyManagement&gt;</code> entry names a version for something this module may not
  even use, so its absence is not a problem; and a plugin, or a dependency that only exists under a
  <code>&lt;profile&gt;</code>, is a warning rather than an error — neither is necessarily fetched
  on a machine that has not run it.
</p>

<h2>Where a version comes from</h2>
<p>
  Reading a pom means opening four files, and every one of those is a jump here.
  <kbd>Ctrl</kbd> + <kbd>B</kbd> on a coordinate goes to the module that builds it when it is one of
  yours, to the parent's <code>&lt;dependencyManagement&gt;</code> entry that pins the version when
  something else decides it, and to the artifact's own <code>.pom</code> in the repository — which
  is where what it drags in is written. On a <code>$&#123;property&#125;</code> it goes to the pom
  that defines it, on the line that does. On a <code>&lt;module&gt;</code>, to that module.
</p>
<p>
  Hover says the same thing without moving: what the coordinate resolves to on disk, what the
  version expands to and who decided it, the scope, and every version of that artifact you have
  installed.
</p>

<h2>When something is missing</h2>
<p>
  Nothing on this page downloads anything — that is what makes it instant and what makes it work
  with no network. <strong>Download dependencies</strong> is the deliberate action that does: it
  runs Maven's <code>dependency:go-offline</code> for the project, reports as a background job, and
  rebuilds the index when it finishes so library types resolve. It is the fix for the state where
  the pom is right and the machine simply does not have the jar yet.
</p>
