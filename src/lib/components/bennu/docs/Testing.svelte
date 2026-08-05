<!-- Bennu docs — running unit tests. -->
<h1>Testing</h1>
<p class="doc-lead">
  Run a whole project, a module, a folder, a class or a single method — and watch the results
  arrive while the run is still going.
</p>

<h2>What counts as a test</h2>
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
<p>
  Discovery reads the file <strong>on disk</strong>. Maven compiles from disk too, so a test you
  have written but not saved is one the runner could not run anyway.
</p>

<h2>Starting a run</h2>
<ul>
  <li><strong>The Tests panel</strong> (<kbd>Alt</kbd> + <kbd>5</kbd>) — every row has a ▷ that runs
    just that class or just that method. Before anything has run, the tree lists what the project
    declares, so the panel is where a run starts as well as where it lands.</li>
  <li><strong>The editor</strong> — <kbd>Ctrl</kbd> + <kbd>Shift</kbd> + <kbd>F10</kbd> runs the test
    the caret is inside; with the caret above the first test method it runs the whole class.</li>
  <li><strong>The project tree</strong> — right-click a folder or a file and pick <em>Run tests</em>.
    The entry appears only where there is something to run, and the count tells you how much.</li>
  <li><strong>The ▷ menu and the command palette</strong> — <em>Run all tests</em>,
    <em>Rerun tests</em>, <em>Rerun failed tests</em>, <em>Stop the test run</em>.</li>
</ul>

<h2>Reading the results</h2>
<p>
  The tree fills in class by class, as each one finishes, rather than all at once at the end — a
  class currently executing shows a spinner. Select any row and the right-hand pane shows its
  failure message, the exception type and the full stack trace; select nothing and it shows the
  raw Maven output, which is what you want while the run is still going.
</p>
<p>
  A ✓ passed, a ✗ failed, a ⚠ errored and a – was skipped. <strong>Failed</strong> and
  <strong>errored</strong> are kept apart on purpose: a failure is a wrong answer — the test ran and
  disagreed with the code — while an error is a broken run that threw before it could judge
  anything. The two are debugged from opposite ends. A test tagged <em>flaky</em> failed and then
  passed on a rerun.
</p>
<p>
  The toolbar filters to failures only, sorts by duration to find what is slow, and offers
  <em>Rerun failed</em> — which re-runs just the cases that went red. A parameterized test reruns as
  a whole declaration, since a failing invocation rarely fails alone.
</p>

<h2>Things worth knowing</h2>
<ul>
  <li>A test run and a build never run at the same time — two Maven processes on one tree fight over
    <code>target/</code>. Starting one while the other is running is refused.</li>
  <li><strong>Stop really stops.</strong> Maven and every process it started are killed, not just
    forgotten about.</li>
  <li>Tests are selected by <strong>simple class name</strong>, which every Maven Surefire version
    understands. Two identically-named test classes in different packages will therefore both run.</li>
  <li>A selection too large to fit on one command line is <strong>widened</strong> to the whole
    project rather than being silently cut short; the panel says so when it happens.</li>
  <li>Unlike the Build button, a test run is not offline: a project that has only ever been compiled
    has no Surefire plugin or test-scope jars cached locally yet.</li>
</ul>
