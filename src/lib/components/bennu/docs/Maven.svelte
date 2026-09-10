<!-- Bennu docs — the Maven tool window: the reactor, its goals, and what a press actually runs. -->
<h1>The Maven tool window</h1>
<p class="doc-lead">
  The project's own poms, as things to run. <kbd>Alt</kbd> + <kbd>8</kbd> opens it, in the same rail
  slot Cargo takes on a Rust project: a project is one or the other, and the key means "the build
  tool" either way.
</p>

<h2>One section per module</h2>
<p>
  A Maven project is a reactor, so the window is one section per module, the root first. Each holds
  two groups:
</p>
<ul>
  <li><strong>Lifecycle</strong> — <code>clean</code>, <code>validate</code>, <code>compile</code>,
    <code>test</code>, <code>package</code>, <code>verify</code>, <code>install</code>,
    <code>site</code>, <code>deploy</code>, plus <code>clean install</code>, which is the one
    combination nobody types out because it is the one everybody runs.</li>
  <li><strong>Plugins</strong> — what that pom configures. A plugin that binds goals lists them, and
    each one runs. A plugin that only carries <code>&lt;configuration&gt;</code> has nothing to
    press — it runs because a lifecycle phase calls it — so its row opens the pom where it is
    declared instead. That is most plugins, and the row says which it is.</li>
</ul>
<p>
  <strong>A phase runs in that module's directory.</strong> Pressing <code>install</code> on
  <code>orders</code> installs <code>orders</code>; pressing it on the root row runs the whole
  reactor, because that is what the root pom's build is. Nothing is added behind your back — no
  <code>-am</code>, no <code>-pl</code> — so what runs is what running <code>mvn</code> in that
  folder would have run.
</p>
<p>
  The module header answers the question the two groups cannot: <strong>where the module is</strong>.
  Its <strong>locate</strong> button — or <strong>Focus in Project</strong> from a right-click on the
  header — opens the Project tree on that module's folder. The same menu opens its
  <code>pom.xml</code> and copies its path.
</p>

<h2>Profiles, and skipping tests</h2>
<p>
  Both belong to the <em>window</em> rather than to a press: you set them once and then run several
  goals.
</p>
<ul>
  <li><strong>Profiles</strong> sit above the modules, because a profile changes what every press
    below it does. They go on as <code>-P</code>. The ones the poms mark
    <code>activeByDefault</code> start ticked — that is what <code>mvn</code> with no
    <code>-P</code> would do, and starting with none ticked would quietly run a different build from
    the terminal. Every other kind of activation Maven supports is a fact about the machine or the
    command line, so the panel does not claim to know whether those hold.</li>
  <li><strong>Skip tests</strong> is the flask in the header. While it is on, every goal run from
    this window carries <code>-DskipTests</code>.</li>
  <li><strong>Offline</strong> is in the actions menu. Off by default: a goal you pressed on purpose
    may legitimately need to fetch the plugin that performs it.</li>
</ul>
<p>
  The choices travel <em>with</em> the run. A console tab's <strong>⟳</strong> repeats the build that
  happened, with the profiles it had — not the one the window is set up for by the time you press it.
</p>

<h2>Where the output goes</h2>
<p>
  Into the <strong>Run console</strong> (<kbd>Alt</kbd> + <kbd>R</kbd>), as its own tab, alongside
  the tabs a <code>java</code> launch and a cargo command make. That is not only tidiness: the tab
  <em>is</em> the run, so <strong>Stop</strong> kills the process tree, the log is annotated like any
  other — levels, paths and stack frames picked out, and a frame clicks through to the file — and
  two runs of the same phase are two transcripts to compare rather than one that overwrote the other.
</p>
<p>
  The tab is named for what ran and where: <code>install · orders</code>. Two modules running the
  same phase are two different builds, and a strip that called both "install" is unreadable from the
  second one on.
</p>

<h2>What the header's actions do</h2>
<p>
  The <strong>refresh</strong> re-reads the poms — this window <em>and</em> the Dependencies panel,
  because they read the same files and a pom that gained a module changed what both should say.
  Everything under the <strong>actions</strong> menu changes what is on disk instead, as background
  jobs that report in the Jobs panel: <strong>Re-resolve dependencies</strong>,
  <strong>Download missing dependencies</strong> (<kbd>Alt</kbd> + <kbd>Shift</kbd> + <kbd>U</kbd>)
  and <strong>Download sources</strong>. See <strong>Dependencies</strong> for what each one fixes.
</p>
<p>
  The <strong>module graph</strong> button (<kbd>Alt</kbd> + <kbd>Shift</kbd> + <kbd>D</kbd>) is here
  too: this window lists the modules, the graph shows how they are wired to each other.
</p>

<h2>What it reads, and what it does not</h2>
<p>
  Building the window costs a parse of every pom in the reactor — no Maven, no network — so it draws
  immediately on a project that has never been built, and the refresh is cheap enough to press
  freely. Maven starts only when you press something.
</p>
<p>
  Which also sets the limit honestly: the goals listed are the ones the poms <em>name</em>. A plugin
  publishes its full goal list inside its own jar, and reading that would mean resolving the plugin
  first — so a goal nobody has bound is not in the list. Any goal at all can still be run from a run
  configuration, or by typing it.
</p>
<p>
  If no <code>mvn</code> can be found on the PATH this app inherits and the project has no
  <code>mvnw</code>, the window says so once at the top rather than letting each press fail to spawn.
</p>
