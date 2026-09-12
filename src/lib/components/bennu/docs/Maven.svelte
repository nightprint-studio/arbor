<script lang="ts">
  /**
   * The Maven tool window: the reactor as things to run, what a press actually runs, the switches that shape every
   * run, and where the output goes.
   */
  import Callout from '$lib/components/shared/ui/Callout.svelte';
</script>

<span class="eyebrow">Projects</span>
<h1>The Maven tool window</h1>

<p class="doc-lead">
  The project's own poms, as things to run. <kbd>Alt</kbd> + <kbd>8</kbd> opens it, in the rail slot Cargo takes on a
  Rust project: a project is one or the other, and the key means "the build tool" either way.
</p>

<h2>One section per module</h2>
<p>
  A Maven project is a reactor, so the window has one section per module, the root first, and each holds two groups:
</p>
<div class="feature-grid two-col">
  <div class="feature-card">
    <div class="fc-eyebrow">What every pom can run</div>
    <div class="fc-title">Lifecycle</div>
    <div class="fc-desc">
      <code>clean</code>, <code>validate</code>, <code>compile</code>, <code>test</code>, <code>package</code>,
      <code>verify</code>, <code>install</code>, <code>site</code>, <code>deploy</code> — plus <code>clean install</code>,
      the one combination nobody types out because it is the one everybody runs.
    </div>
  </div>
  <div class="feature-card">
    <div class="fc-eyebrow">What that pom configures</div>
    <div class="fc-title">Plugins</div>
    <div class="fc-desc">
      A plugin that binds goals lists them, and each one runs. A plugin that only carries
      <code>&lt;configuration&gt;</code> has nothing to press — a lifecycle phase calls it — so its row opens the pom that
      declares it instead. That is most plugins, and the row says which kind it is.
    </div>
  </div>
</div>

<h2>What a press runs</h2>
<p>
  <strong>A phase runs in its module's directory.</strong> Pressing <code>install</code> on <code>orders</code> installs
  <code>orders</code>; pressing it on the root row runs the whole reactor, because that is what the root pom's build is.
</p>
<Callout variant="info" title="Nothing added behind your back">
  No <code>-am</code>, no <code>-pl</code>: what runs is what running <code>mvn</code> in that folder would have run.
</Callout>
<p>
  The module header answers what the two groups cannot — <strong>where the module is</strong>. Its
  <strong>locate</strong> button, or <strong>Focus in Project</strong> from a right-click on the header, opens the Project
  tree on the module's folder. The same menu opens its <code>pom.xml</code> and copies its path.
</p>

<h2>Profiles, and skipping tests</h2>
<p>
  These belong to the <em>window</em> rather than to one press: you set them once, then run several goals.
</p>
<dl class="meta-grid">
  <dt>Profiles</dt>
  <dd>
    Above the modules, because a profile changes what every press below it does. They are passed as <code>-P</code>. The
    ones the poms mark <code>activeByDefault</code> start ticked — that is what <code>mvn</code> with no <code>-P</code>
    does, and starting with none ticked would quietly run a different build from the terminal's. Every other kind of
    activation Maven supports is a fact about the machine or the command line, so the panel does not claim to know
    whether it holds.
  </dd>
  <dt>Skip tests</dt>
  <dd>The flask in the header. While it is on, every goal run from this window carries <code>-DskipTests</code>.</dd>
  <dt>Offline</dt>
  <dd>In the actions menu, and off by default: a goal you pressed on purpose may need to fetch the plugin that performs it.</dd>
</dl>
<Callout variant="tip" title="The choices travel with the run">
  A console tab's <strong>⟳</strong> repeats the build that happened, with the profiles it had — not the ones the window
  is set to by the time you press it.
</Callout>

<h2>Where the output goes</h2>
<p>
  Into the <strong>Run console</strong> (<kbd>Alt</kbd> + <kbd>R</kbd>), as its own tab beside the tabs a
  <code>java</code> launch and a cargo command make. The tab <em>is</em> the run:
</p>
<ul>
  <li><strong>Stop</strong> kills the process tree.</li>
  <li>The log is annotated like any other — levels, paths and stack frames picked out, and a frame clicks through to the file.</li>
  <li>Two runs of the same phase are two transcripts to compare, not one that overwrote the other.</li>
</ul>
<p>
  The tab is named for what ran and where — <code>install · orders</code>. Two modules running the same phase are two
  different builds, and a strip that called both "install" would be unreadable from the second one on.
</p>

<h2>The header's actions</h2>
<table>
  <thead><tr><th>Action</th><th>What it does</th></tr></thead>
  <tbody>
    <tr>
      <td><strong>Refresh</strong></td>
      <td>Reads the poms again — for this window <em>and</em> the Dependencies panel, because they read the same files and a
        pom that gained a module changed what both should say.</td>
    </tr>
    <tr>
      <td><strong>Re-resolve dependencies</strong></td>
      <td rowspan="3">Under the <strong>actions</strong> menu: these change what is on disk, as background jobs that report in the
        Jobs panel. <strong>Dependencies</strong> says what each one fixes.</td>
    </tr>
    <tr><td><strong>Download missing dependencies</strong> <kbd>Alt</kbd> + <kbd>Shift</kbd> + <kbd>U</kbd></td></tr>
    <tr><td><strong>Download sources</strong></td></tr>
    <tr>
      <td><strong>Module graph</strong> <kbd>Alt</kbd> + <kbd>Shift</kbd> + <kbd>D</kbd></td>
      <td>This window lists the modules; the graph shows how they are wired to each other.</td>
    </tr>
  </tbody>
</table>

<h2>What it reads, and what it does not</h2>
<p>
  Building the window costs a parse of every pom in the reactor — no Maven, no network — so it draws at once on a project
  that has never been built, and the refresh is cheap enough to press freely. Maven starts only when you press something.
</p>
<Callout variant="info" title="Only the goals the poms name">
  A plugin publishes its full list of goals inside its own jar, and reading that would mean resolving the plugin first —
  so a goal nobody has bound is not listed. Any goal at all can still be run from a run configuration, or by typing it.
</Callout>
<p>
  When no <code>mvn</code> can be found on the <code>PATH</code> this app inherits and the project has no
  <code>mvnw</code>, the window says so once at the top, rather than letting every press fail to start.
</p>
