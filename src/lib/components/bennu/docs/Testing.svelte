<script lang="ts">
  /**
   * Testing: what counts as a test in Java and Rust, the catalogue and the run, the tree, starting and debugging a run, the
   * silent failures (engines, a pom pinning <test>), reading results, and what is worth knowing.
   */
  import Callout from '$lib/components/shared/ui/Callout.svelte';
  import { highlightCode } from '$lib/utils/highlight';
</script>

<span class="eyebrow">Build, run &amp; test</span>
<h1>Testing</h1>

<p class="doc-lead">
  Run a whole project, a crate, a module, a folder, a class or a single test — and watch the results arrive while the run is still going.
</p>
<p>
  The runner follows the project: <code>mvn test</code> and Surefire on Maven, <code>cargo test</code> on a Cargo workspace. The panel, the shortcuts and the rerun buttons are the same;
  what differs is how a test is named and how deep the tree goes.
</p>

<h2>What counts as a test</h2>
<div class="feature-grid two-col">
  <div class="feature-card">
    <div class="fc-eyebrow">Read from the sources, not file names</div>
    <div class="fc-title">Java</div>
    <div class="fc-desc">A method with <code>@Test</code> — or JUnit 5's <code>@ParameterizedTest</code>, <code>@RepeatedTest</code>, <code>@TestFactory</code>, <code>@TestTemplate</code> — a <code>public void testXxx()</code> in a JUnit 3 <code>TestCase</code>, or, under TestNG, any public method of a class annotated <code>@Test</code>. Setup and teardown are never tests.</div>
  </div>
  <div class="feature-card">
    <div class="fc-eyebrow">An attribute ending in <code>test</code></div>
    <div class="fc-title">Rust</div>
    <div class="fc-desc"><code>#[test]</code>, and equally <code>#[tokio::test]</code>, <code>#[sqlx::test]</code>, <code>#[actix_web::test]</code> or the next runtime's — a closed list would miss the next. <code>#[bench]</code> counts; <code>#[rstest]</code> and <code>#[test_case(…)]</code> are marked <em>cases</em>, producing many.</div>
  </div>
</div>
<ul>
  <li>That is why a helper called <code>OrderTestUtils</code> does not appear, and a JUnit 3 class named anything at all does. An abstract base holding shared tests is shown but cannot run
    alone — Surefire instantiates concrete classes.</li>
  <li><code>#[ignore]</code> shows as skipped and runs only when asked: the <em>eye</em> toggle adds <code>--include-ignored</code>. <code>#[should_panic]</code> is badged — the one row that
    passes <em>by</em> failing.</li>
  <li>Discovery reads the file <strong>on disk</strong>: both build systems compile from disk, so an unsaved test is one the runner could not run anyway.</li>
</ul>

<h2>Two places, and why</h2>
<dl class="meta-grid">
  <dt>The Tests tool window</dt>
  <dd><kbd>Alt</kbd> + <kbd>5</kbd>, right rail: the <em>catalogue</em> — everything the sources declare, with a filter and a ▷ on every row. On a Cargo workspace it also fills with verdicts as a run streams. Its header folds the whole tree either way, which on twenty crates is the view you can read.</dd>
  <dt>A run</dt>
  <dd>In the <strong>Run</strong> console (<kbd>Alt</kbd> + <kbd>R</kbd>), as a tab beside your launched programs, because a test run <em>is</em> a launch: a command, a live transcript, a Stop, an outcome. Its transcript is the console's own — interpreted, with clickable frames.</dd>
</dl>

<h2>The tree</h2>
<table>
  <thead><tr><th>Runner</th><th>Levels</th></tr></thead>
  <tbody>
    <tr><td>Java</td><td>class → case</td></tr>
    <tr><td>Rust</td><td>crate → target → module → test</td></tr>
  </tbody>
</table>
<p>
  The Rust levels are not decoration: <code>tests::works</code> means nothing in a workspace where twenty crates have one. Every level runs — the ▷ on a crate, a target or a module runs it and
  everything under it.
</p>
<ul>
  <li>A <em>target</em> is what cargo builds and runs as one binary — the <code>lib</code>, each <code>bin</code>, each file under <code>tests/</code>, each bench — and the unit that reports a
    duration, so timings sit on target rows. libtest does not time cases without an unstable flag, and a divided-out figure would be invented.</li>
  <li>Rust rows are there <strong>before</strong> anything runs, and turn green, red or grey as results arrive. A case no declaration matches is added, not dropped — how an
    <code>#[rstest]</code>'s generated cases appear.</li>
</ul>

<h2>Starting a run</h2>
<table>
  <thead><tr><th>From</th><th>Runs</th></tr></thead>
  <tbody>
    <tr><td>The Tests window's ▷</td><td>That row. An abstract or disabled Java class has no ▷: Surefire cannot instantiate the first, and the second would report skipped.</td></tr>
    <tr><td>The ▶ in a Java file's gutter</td><td><em>Run</em> or <em>Debug</em> for that class or case. A disabled test keeps its arrow and says so. Arrows come from the file on disk, so an unsaved <code>@Test</code> has none.</td></tr>
    <tr><td><kbd>Ctrl</kbd> + <kbd>Shift</kbd> + <kbd>F10</kbd> in the editor</td><td>The test the caret is in; above the first test, the class (Java) or the file's target (Rust)</td></tr>
    <tr><td><em>Run tests</em> on a folder or file in the project tree</td><td>What is under it — offered only where there is something, with the count</td></tr>
    <tr><td><kbd>Ctrl</kbd> + <kbd>Shift</kbd> + <kbd>F5</kbd></td><td>Every test</td></tr>
    <tr><td><kbd>Ctrl</kbd> + <kbd>F5</kbd></td><td>What ran last</td></tr>
    <tr><td>The ▷ menu, the command palette</td><td><em>Run all tests</em>, <em>Rerun tests</em>, <em>Rerun failed tests</em>, <em>Stop the test run</em></td></tr>
  </tbody>
</table>

<h2>Debugging a test</h2>
<p>
  Every ▷ in the Tests panel has a <strong>bug icon</strong> beside it running the same thing under the debugger. Nothing to attach, no port: the forked test JVM dials back to Bennu and suspends
  until it arrives, so the breakpoints in your buffer are hit on the first run.
</p>
<p>
  When one is hit, the run's tab becomes the debugger — transport controls on its status row, frames and variables in the two columns left of the tree — and comes forward on its own.
</p>
<Callout variant="info" title="Stop ends the session as well as the run">
  The forked JVM is Maven's child, not Maven, so Stop takes the whole process group — otherwise a test suspended at a breakpoint would outlive its run, and the editor would keep a current line in a
  program nothing could resume.
</Callout>
<p>
  A project with <code>&lt;forkCount&gt;0&lt;/forkCount&gt;</code> runs tests in Maven's own JVM, with no forked process to debug — that run is refused with the reason. A debug run is forced to a
  single fork, since one listener accepts one connection.
</p>

<h2>Test classes that are never run</h2>
<p>
  The JUnit Platform runs <em>engines</em>, one per dialect: <code>junit-jupiter-engine</code> for JUnit 5, <code>junit-vintage-engine</code> for JUnit 4. A project migrated to Jupiter without
  the vintage engine still <strong>compiles</strong> every JUnit 4 test, and Surefire simply never runs them.
</p>
<Callout variant="warning" title="A build that passes, and a hundred classes that did not run">
  Surefire reports on what it <em>ran</em>, so nothing says so — except the Tests panel, above the tree, with the count and a class name. The other way is just as quiet: only the vintage engine
  means the JUnit 5 tests are the skipped ones.
</Callout>
<p>
  Nothing is claimed before the classpath resolves — an empty jar list means not looked yet — and nothing at all on a plain JUnit 4 build, which never loads the Platform.
</p>

<h2>A pom that pins which tests run</h2>
<p>
  A Surefire plugin configured with a <strong>literal</strong> <code>&lt;test&gt;</code> makes a selection impossible: Maven gives the pom's value precedence over <code>-Dtest</code>, so running
  one class is read and discarded. The Tests panel says so above the tree, before anything runs, and a run that went ahead is marked <em>widened</em>.
</p>
<p>Written as a <strong>property</strong>, it works and keeps its default:</p>
<pre><code>{@html highlightCode(`<properties>
    <suite>TestSuite</suite>
</properties>
…
<plugin>
    <artifactId>maven-surefire-plugin</artifactId>
    <configuration>
        <test>\${suite}</test>
    </configuration>
</plugin>`, 'markup')}</code></pre>
<p>
  A plain <code>mvn test</code> still runs the suite, while a run started here sets that property and gets the class or case you picked. Any property name does — Bennu reads the one your pom
  uses; <code>&#36;&#123;test&#125;</code> is the case where it is Surefire's own.
</p>
<Callout variant="tip" title="Convert to a property">
  The button on that warning makes the change: the pinned value becomes a property and <code>&lt;test&gt;</code> a reference to it, named <code>test</code> whenever the pom leaves that name free —
  steerable from a bare terminal and any tool. You see which pom will be written first, and only that line changes; comments, order and indentation stay.
</Callout>

<h2>Reading the results</h2>
<p>
  The tree fills as the run goes — class by class under Maven, test by test under cargo. Select a row and the right pane shows its failure message, exception type and stack trace (Java) or its
  captured panic (Rust); select nothing and it shows the runner's output, interpreted like the Run console's.
</p>
<table>
  <thead><tr><th>Mark</th><th>Means</th></tr></thead>
  <tbody>
    <tr><td>✓</td><td>Passed</td></tr>
    <tr><td>✗</td><td>Failed — the test ran and disagreed with the code</td></tr>
    <tr><td>⚠</td><td>Errored — it threw before it could judge anything (Maven only; libtest has no such distinction)</td></tr>
    <tr><td>–</td><td>Skipped</td></tr>
    <tr><td><em>flaky</em></td><td>Failed, then passed on a rerun</td></tr>
  </tbody>
</table>
<p>
  Failed and errored are kept apart on purpose: they are debugged from opposite ends. The toolbar filters to failures, sorts by duration, and offers <em>Rerun failed</em> — a parameterized test reruns
  as a whole declaration, since a failing invocation rarely fails alone. While cargo compiles, the panel names the crate it is on.
</p>

<h2>Things worth knowing</h2>
<ul>
  <li>A test run and a build never overlap — two build processes on one tree fight over the output directory — and starting one during the other is refused.</li>
  <li><strong>Stop really stops</strong>: the build tool and everything it started are killed.</li>
  <li>Java tests are selected by <strong>simple class name</strong>, which every Surefire understands — so two same-named classes in different packages both run.</li>
  <li>Rust tests are narrowed on <em>both</em> sides — cargo picks the binaries (<code>-p</code>, <code>--lib</code>, <code>--test</code>) and the binary filters by name — so one test means one test.</li>
  <li>A cargo run passes <code>--no-fail-fast</code>: otherwise the first failing crate ends the run and every later crate reports nothing.</li>
  <li>A selection too large for one command line is <strong>widened</strong>, not silently cut, and the panel says so.</li>
  <li>Neither runner is offline: a project only ever compiled may lack its test-only dependencies, and an offline failure there would read as a Bennu bug.</li>
  <li>Doc tests are run by <code>cargo test</code> itself, under a <em>doc-tests</em> target.</li>
</ul>
