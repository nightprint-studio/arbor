<!-- Bennu docs — running unit tests, for a Maven project and for a Cargo workspace. -->
<h1>Testing</h1>
<p class="doc-lead">
  Run a whole project, a crate, a module, a folder, a class or a single test — and watch the
  results arrive while the run is still going.
</p>
<p>
  Which runner you get follows the project: a Maven project runs through <code>mvn test</code> and
  Surefire, a Cargo workspace through <code>cargo test</code>. The panel, the shortcuts and the
  Rerun buttons are the same either way; what differs is how a test is named and how deep the tree
  goes, and both of those are described below.
</p>

<h2>What counts as a test</h2>
<h3>Java</h3>
<p>
  Bennu reads your sources rather than trusting file names. A test is a method carrying
  <code>@Test</code> (or JUnit 5's <code>@ParameterizedTest</code>, <code>@RepeatedTest</code>,
  <code>@TestFactory</code>, <code>@TestTemplate</code>), a <code>public void testXxx()</code>
  inside a JUnit 3 class extending <code>TestCase</code>, or — under TestNG — any public method of
  a class annotated <code>@Test</code>. Setup and teardown methods are never listed as tests.
</p>
<p>
  This is why a helper called <code>OrderTestUtils</code> does not appear, and why a JUnit 3 class
  named anything at all does. Abstract base classes holding shared test methods are shown but
  cannot be run on their own — Surefire instantiates concrete classes.
</p>

<h3>Rust</h3>
<p>
  A function is a test when an attribute above it <strong>ends in <code>test</code></strong>:
  <code>#[test]</code>, and equally <code>#[tokio::test]</code>, <code>#[sqlx::test]</code>,
  <code>#[actix_web::test]</code> or the next async runtime's. A closed list would have to grow
  every time one appears, and a missing entry means a test the panel silently does not know about.
  <code>#[bench]</code> counts too, and <code>#[rstest]</code> / <code>#[test_case(…)]</code> are
  marked <em>cases</em> because one such function produces many.
</p>
<p>
  <code>#[ignore]</code> shows as skipped and is only run when you ask: the <em>eye</em> toggle in
  the panel's header adds <code>--include-ignored</code> to the run.
  <code>#[should_panic]</code> is badged, since it is the one row that passes <em>by</em> failing.
</p>
<p>
  Discovery reads the file <strong>on disk</strong>. Both build systems compile from disk too, so a
  test you have written but not saved is one the runner could not run anyway.
</p>

<h2>Two places, and why</h2>
<p>
  <strong>The Tests tool window</strong> (<kbd>Alt</kbd> + <kbd>5</kbd>, right rail) is the
  <em>catalogue</em>: everything the project declares, with a filter and a ▷ on every row. It is a
  property of your sources — stable, browsable, and where a run starts. On a Cargo workspace it
  also fills with verdicts as a run streams, so it doubles as a status column.
</p>
<p>
  Its header folds the whole tree either way — the same two buttons the run console carries. On a
  workspace of twenty crates the folded view is the one you can read, and getting back to it after
  opening half a dozen modules is one press.
</p>
<p>
  <strong>A run</strong> lands in the <strong>Run</strong> console (<kbd>Alt</kbd> + <kbd>R</kbd>),
  as a tab beside the programs you have launched, because a test run <em>is</em> a launch: a
  command, a live transcript, a Stop button, an outcome. The tab appears when you start one and
  closes when you are done with it, like every other tab there. Its transcript is the console's own
  — interpreted, with clickable stack frames, and rendering only what is on screen however long the
  run gets.
</p>

<h2>The tree</h2>
<p>
  A Java run is <strong>class → case</strong>. A Rust run is
  <strong>crate → target → module → test</strong>, and the extra levels are not decoration: a row
  reading <code>tests::works</code> is meaningless in a workspace of twenty crates, because twenty
  of them have one. Every level of the tree is also something you can run — the ▷ on a crate row
  runs that crate, on a target row that target, on a module row that module and everything under
  it.
</p>
<p>
  A <em>target</em> is what cargo builds and runs as one binary: the crate's <code>lib</code>, each
  <code>bin</code>, each file under <code>tests/</code>, each bench. That is also the unit that
  reports a duration, so timings sit on target rows rather than on individual tests — libtest does
  not time cases individually without an unstable flag, and a per-test number divided out of the
  block total would be an invention.
</p>
<p>
  Rust rows are there <strong>before</strong> anything runs and turn green, red or grey as their
  results arrive. A case the run reports that no declaration matches is added rather than dropped:
  that is how an <code>#[rstest]</code>'s generated cases appear, and how a test the scan did not
  recognise still lands in the panel.
</p>

<h2>Starting a run</h2>
<ul>
  <li><strong>The Tests tool window</strong> (<kbd>Alt</kbd> + <kbd>5</kbd>) — every row has a ▷,
    and expanding one gives a ▷ per child. An abstract or disabled Java class is listed but has no
    ▷: Surefire cannot instantiate the first, and the second would report as skipped.</li>
  <li><strong>The editor's gutter</strong> — a ▶ beside every test class and every test case in a
    Java file. Pressing it (or right-clicking it) opens <em>Run</em> and <em>Debug</em> for that
    one thing. A disabled test keeps its arrow and says so: Surefire runs what it is told and
    reports it skipped, which is occasionally what you want to confirm. The arrows come from the
    file <strong>on disk</strong> — a <code>@Test</code> you have typed but not saved is one the
    runner cannot see, so it gets no arrow until the save.</li>
  <li><strong>The editor</strong> — <kbd>Ctrl</kbd> + <kbd>Shift</kbd> + <kbd>F10</kbd> runs the test
    the caret is inside, without the menu. Above the first test it runs the whole class (Java) or
    the file's target (Rust).</li>
  <li><strong>The project tree</strong> — right-click a folder or a file and pick <em>Run tests</em>.
    The entry appears only where there is something to run, and the count tells you how much.</li>
  <li><strong>The ▷ menu and the command palette</strong> — <em>Run all tests</em>,
    <em>Rerun tests</em>, <em>Rerun failed tests</em>, <em>Stop the test run</em>. Also
    <kbd>Ctrl</kbd> + <kbd>Shift</kbd> + <kbd>F5</kbd> to run everything and
    <kbd>Ctrl</kbd> + <kbd>F5</kbd> to rerun the last run.</li>
</ul>

<h2>Debugging a test</h2>
<p>
  Every ▷ in the Tests panel has a <strong>bug icon</strong> beside it that runs the same thing
  under the debugger. There is nothing to attach and no port to choose: the forked test JVM dials
  back to Bennu and suspends until it arrives, so the breakpoints already in your buffer are hit on
  the first run.
</p>
<p>
  When one is hit, the <strong>Run</strong> panel's test tab becomes the debugger: the transport
  controls — resume, step over, step into, step out, detach — appear on its status row, and the
  stopped JVM's <strong>frames</strong> and <strong>variables</strong> take the two columns to the
  left of the tree, in the same places and with the same collapse strips they have beside a
  program's transcript. The tab comes forward on its own when a test stops, the way the window
  does.
</p>
<p>
  <strong>Stop ends the session as well as the run.</strong> The forked JVM is a child of Maven
  rather than Maven itself, so stopping takes the whole process group — otherwise a test suspended
  at a breakpoint would outlive the run that started it, and the editor would keep showing a
  current line in a program nothing could resume.
</p>
<p>
  A project that sets <code>&lt;forkCount&gt;0&lt;/forkCount&gt;</code> on Surefire runs its tests
  inside Maven's own JVM, where there is no forked process to debug — that run is refused with the
  reason rather than started without a debugger. A debug run is forced to a single fork, since one
  listener accepts one connection.
</p>

<h2>Test classes that are never run</h2>
<p>
  The JUnit Platform runs <em>engines</em>, one per dialect: <code>junit-jupiter-engine</code> for
  JUnit 5's <code>@Test</code>, <code>junit-vintage-engine</code> for JUnit 4's. A project migrated
  to Jupiter that forgot the vintage engine still <strong>compiles</strong> every JUnit 4 test it
  has — the annotations resolve, the assertions resolve — and Surefire simply never executes them.
</p>
<p>
  What you see is a build that passes. What is true is that a hundred test classes did not run, and
  nothing reports it, because Surefire reports on what it <em>ran</em>. The Tests panel says so above
  the tree, with the count and one of the class names. It goes the other way too, and is just as
  quiet: only the vintage engine means the JUnit 5 tests are the ones being skipped.
</p>
<p>
  Nothing is claimed before the classpath has resolved — an empty jar list means we have not looked
  yet, not that an engine is missing — and nothing at all on a plain JUnit 4 build, which never loads
  the Platform and runs its tests exactly as it always did.
</p>

<h2>A pom that pins which tests run</h2>
<p>
  A Surefire plugin configured with a <strong>literal</strong> <code>&lt;test&gt;</code> makes a
  selection impossible: Maven gives a value written in the pom precedence over the
  <code>-Dtest</code> that names it, so running one class or one case is read and discarded and the
  run does whatever the pom said. The Tests panel says so above the tree, before anything is run —
  it is a fact about every ▷ in the panel, not about one click — and a run that went ahead anyway
  is marked <em>widened</em>.
</p>
<p>
  Written as a <strong>property</strong> it works, and the default is kept:
  <code>&lt;test&gt;&#36;&#123;suite&#125;&lt;/test&gt;</code> with <code>&lt;suite&gt;TestSuite&lt;/suite&gt;</code>
  in <code>&lt;properties&gt;</code> means a plain <code>mvn test</code> still runs the suite, while
  a run started from here sets that same property and gets the one class or case you picked. Any
  property name does — Bennu reads the one your pom uses. <code>&#36;&#123;test&#125;</code> is the special case
  of that where the name happens to be Surefire's own.
</p>
<p>
  <strong>Convert to a property</strong> on that warning makes the change for you: the pom declares
  the pinned value as a property and its <code>&lt;test&gt;</code> becomes a reference to it. It
  names the property <code>test</code> whenever the pom leaves that name free, so the project is
  steerable from a bare terminal and from any other tool as well as from here. You are shown which
  pom will be written before anything is, and only that one line of it changes — comments, ordering
  and indentation are left exactly as they were.
</p>

<h2>Reading the results</h2>
<p>
  The tree fills in as the run goes — class by class under Maven, test by test under cargo — rather
  than all at once at the end. Select any row and the right-hand pane shows its failure message,
  the exception type and the stack trace (Java) or the captured panic (Rust); select nothing and it
  shows the runner's output, which is what you want while the run is still going. That output is
  interpreted the way the Run console's is — levels coloured, and a stack frame in a class of this
  project is a link to the line it names.
</p>
<p>
  A ✓ passed, a ✗ failed, a ⚠ errored and a – was skipped. Under Maven, <strong>failed</strong> and
  <strong>errored</strong> are kept apart on purpose: a failure is a wrong answer — the test ran and
  disagreed with the code — while an error is a broken run that threw before it could judge
  anything. The two are debugged from opposite ends. libtest draws no such distinction, so a Rust
  test is failed or it is not. A test tagged <em>flaky</em> failed and then passed on a rerun.
</p>
<p>
  The toolbar filters to failures only, sorts by duration to find what is slow, and offers
  <em>Rerun failed</em> — which re-runs just the cases that went red. A parameterized test reruns as
  a whole declaration, since a failing invocation rarely fails alone.
</p>
<p>
  While cargo is compiling, the panel says which crate it is on. On a cold workspace that is the
  first several seconds of a run, and nothing else has anything to report yet.
</p>

<h2>Things worth knowing</h2>
<ul>
  <li>A test run and a build never run at the same time — two build processes on one tree fight over
    the output directory. Starting one while the other is running is refused.</li>
  <li><strong>Stop really stops.</strong> The build tool and every process it started are killed,
    not just forgotten about.</li>
  <li>Java tests are selected by <strong>simple class name</strong>, which every Maven Surefire
    version understands. Two identically-named test classes in different packages will therefore
    both run.</li>
  <li>Rust tests are narrowed on <em>both</em> sides — cargo chooses which binaries to run
    (<code>-p</code>, <code>--lib</code>, <code>--test</code>) and the binary itself filters by
    name. That is what makes "run this one test" mean one test rather than every same-named test in
    the workspace.</li>
  <li>A cargo run always passes <code>--no-fail-fast</code>: without it the first crate that fails
    ends the run and every later crate reports nothing, which on a large workspace turns one broken
    crate into silence about all the others.</li>
  <li>A selection too large to fit on one command line is <strong>widened</strong> rather than being
    silently cut short; the panel says so when it happens.</li>
  <li>Neither runner is offline: a project that has only ever been compiled may not have its
    test-only dependencies cached yet, and an offline failure there reads as a bug in Bennu rather
    than as a missing download.</li>
  <li>Doc tests are run by <code>cargo test</code> itself and appear under a
    <em>doc-tests</em> target when they do.</li>
</ul>
