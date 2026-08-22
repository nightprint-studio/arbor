<!-- Bennu docs — building, running an application, and the Run console. -->
<h1>Building &amp; running</h1>
<p class="doc-lead">
  Compile the project, launch its <code>main</code>, and watch — and answer — its output without
  leaving the window.
</p>

<h2>Building</h2>
<p>
  <kbd>Ctrl</kbd> + <kbd>F9</kbd> compiles. On a Maven project that is <code>mvn compile</code>
  under the JDK the project declares; on a Cargo project it is <code>cargo check</code>, because
  what the button is for is the diagnostics and <code>check</code> reaches them without linking.
  The split button beside it offers <strong>Validate (no compile)</strong>, which runs the
  editor's own analysis over every file — seconds instead of a full build, at the cost of
  answering only the questions an editor can answer.
</p>
<p>
  Output lands in the <strong>Build</strong> panel (<kbd>Alt</kbd> + <kbd>0</kbd>): the parsed
  errors first, each one clickable straight to its line, then the log — interpreted the same
  way the Run console's is, so Maven's <code>[ERROR]</code>s stand out and the paths and stack
  frames in it are links. A clean Java build re-indexes <code>target/classes</code>, so
  completion sees what you just compiled.
</p>
<h2>Run configurations</h2>
<p>
  A run configuration is a named launch target. They live in
  <code>&lt;project&gt;/.arbor/bennu/config.toml</code>, so they are per project, survive a restart, and
  can be committed for everyone or ignored — your choice, it is a file.
</p>
<p>
  The <strong>selector in the title bar</strong>, left of ▷, says which one those buttons will
  start, and opens the list — grouped by category — to change it. Picking one there makes it
  <strong>active</strong>; it does not launch it. Choosing what ▷ means and pressing ▷ are
  separate acts.
</p>

<h3>Categories</h3>
<p>
  Adding a configuration asks what kind first, and the editor's list is grouped the same way:
</p>
<ul>
  <li><strong>Application</strong> — a module, a <code>main</code> class, and program
    arguments, VM arguments, a working directory and environment variables. The
    <strong>main class</strong> field has a <em>Choose</em> picker listing the classes in that
    module declaring <code>public static void main(String[])</code> — you should not have to
    type a fully-qualified name the editor already knows — and choosing one fills the module
    in for you.</li>
  <li><strong>Spring Boot</strong> — the same launch plus its <strong>active profiles</strong>,
    which become <code>-Dspring.profiles.active=…</code>. The <em>Detected</em> picker lists the
    profiles the project declares, read from its own <code>application-&lt;profile&gt;.yml</code>
    and <code>.properties</code> files; the field stays free text, because a profile can be
    declared inside a YAML document or not exist yet. <strong>The main class is
    optional</strong>: left empty, the module's <code>@SpringBootApplication</code> is what
    starts — a Boot module has exactly one, so naming it changes nothing. Offered only on a
    project that has Spring.</li>
  <li><strong>JUnit</strong> — a test scope: the whole project, one module, or one class. It runs
    through the test runner: it lands here too, as its own tab, with the test tree beside the output.</li>
  <li><strong>Cargo</strong> — a cargo subcommand: a crate, a command, a target, features, a
    profile, and two argument fields (<strong>Cargo arguments</strong> before the <code>--</code>,
    <strong>Program arguments</strong> after it). Everything but the arguments and the working
    directory is a picker, because the workspace already knows its crates, their targets and their
    features. Offered only on a Cargo project — and the JVM categories only on a Java one.</li>
</ul>
<p>
  On a project with exactly one entry point you need none of this: press ▷ and Bennu finds it,
  makes a configuration for it — a Spring Boot one if that class is a Boot application — and
  runs it. With several, it opens the editor and asks, which is a real question. A Cargo workspace
  with exactly one binary works the same way, and gets a Cargo configuration for it.
</p>
<p>
  A Cargo configuration has <strong>no build step in front of it</strong> — the command <em>is</em>
  the build, so prefixing it with one would compile the workspace twice. Everything after that is
  identical: the same console, the same Stop, the same ⟳. 🐞 is the exception, and stays JVM-only:
  it attaches JDWP to a JVM Bennu started, and a cargo command forks its own compiler and its own
  program. See <em>Rust &amp; Cargo</em>.
</p>

<h3>Multi-module projects</h3>
<p>
  The <strong>module</strong> is the first field of a configuration because it decides the
  classpath, and on a reactor the root usually compiles nothing at all — a run configured
  without one launches against a directory that does not exist. The classpath is that module's
  <code>target/classes</code> first, then every other module's, then the dependencies: your
  inner loop is compile-and-run without <code>mvn install</code>, so a call across a module
  boundary has to find the sibling's classes where they actually are.
</p>
<p>
  The dependencies are resolved at the <strong>runtime</strong> scope — what
  <code>mvn spring-boot:run</code> and a packaged application see. That is deliberately
  <em>narrower</em> than what the editor uses: indexing, completion and navigation resolve every
  scope, because you edit tests and they have dependencies of their own. Launching with that
  wider classpath hands the JVM libraries Maven would never supply, and the difference is not
  cosmetic — a <code>@ConditionalOnClass</code> guarding a bean on a test-scoped library then
  fires here and nowhere else, and the application refuses to start in the IDE while Maven is
  perfectly happy.
</p>
<p>
  <strong>Classpath</strong> in the configuration changes it, per configuration: Compile, Test,
  or every scope. The exceptions are real — a launcher that wants a test-scoped H2 or a provided
  servlet API is a legitimate thing to want — so it is a choice rather than a reason to stop
  using the run panel. The first launch of a configuration resolves its classpath through Maven;
  every one after it is instant until the pom changes.
</p>
<p>
  The working directory defaults to the module's own directory, and the editor's list and the
  title-bar selector both show which module a configuration is for — otherwise four
  configurations called <em>Application</em> are the same row four times.
</p>
<p>
  Entirely from the keyboard: the palette's <em>Edit run configuration…</em> opens the editor with
  the list focused, ↑/↓ move, the ● button makes one active, <kbd>Enter</kbd> runs it and
  <kbd>Ctrl</kbd> + <kbd>Enter</kbd> closes. Nothing needs saving — every edit is written as you
  type.
</p>
<h2>The Run console</h2>
<p>
  <kbd>Shift</kbd> + <kbd>F10</kbd> builds and then launches, and the <strong>Run</strong> panel
  (<kbd>Alt</kbd> + <kbd>R</kbd>) opens on the program's own output. It is a separate tool window
  from Build on purpose: a build log is finished by the time you read it, while a program's
  output is a live thing you watch, type into and stop.
</p>
<ul>
  <li><strong>The command that ran</strong> is the first line — the resolved <code>java</code>,
    the VM arguments and the class, quoted so it can be pasted into a terminal. The classpath is
    summarised as a count, since the real one is tens of thousands of characters. On a real
    dependency tree it does not fit on a command line at all (Windows caps one at 32 767
    characters), so it is handed over in a JDK <strong>argument file</strong> —
    <code>@&lt;module&gt;/target/bennu-run.args</code>, which is what the line then shows and
    is still exactly what you can paste. On a Java 8, which has no argument files, it goes
    through <code>CLASSPATH</code> instead.</li>
  <li><strong>You can type back.</strong> While a program runs, a <code>&gt; Send input</code>
    strip sits at the bottom; opening it gives you a line that writes to the program's standard
    input, so something that stops at a prompt can be answered instead of appearing to hang.
    What you send is echoed with a <code>&gt;</code>, the way a terminal echoes it.
    <kbd>Esc</kbd> puts it away. It starts closed on every run — most programs never read a
    line, and Bennu cannot tell from outside whether yours is waiting for one.</li>
  <li><strong>Stop really stops</strong> — the whole process tree, not just the process Bennu
    launched. A program that spawns children does not outlive the button.</li>
  <li><strong>It ends with a verdict</strong>: the exit code and how long the run took.</li>
  <li><strong>The output is read, not just printed.</strong> Every line is interpreted into
    what its parts are — the level, the timestamp, the thread, the logger, an exception name,
    a URL, a path — and coloured accordingly, so the one line that matters is findable without
    reading the ninety around it. Levels are recognised in upper case
    (<code>ERROR</code>, <code>[WARN]</code>, <code>SEVERE</code>): a viewer that painted the
    word <em>Error</em> in a sentence red would be a viewer whose colours mean nothing.</li>
  <li><strong>What a line says beats where it came from.</strong> A compiler writes its warnings
    to standard error — <code>rustc</code>, <code>cargo</code>, <code>javac</code> and
    <code>gcc</code> all do — so colouring by stream paints a warning exactly the colour of an
    error, and then has nothing left to say when something is actually wrong. A
    <code>warning:</code> at the start of a line is amber, an <code>error:</code> is red, and
    rustc's <code>note:</code> and <code>help:</code> are the quiet colour they deserve: they are
    halves of one diagnostic, not four problems. The source excerpt underneath — the
    <code>--&gt;</code>, the bars, the numbered line — stays part of the diagnostic above it.</li>
  <li><strong>Red means the line said it was an error</strong>, and nothing else does. A line
    nobody could interpret is neutral whichever pipe it came down: <code>cargo</code> writes its
    <em>entire</em> log to standard error — <code>Compiling</code>, <code>Finished</code>,
    <code>Running</code> — and a console where all of that is red is a console with no colours
    left. The two failures that carry no level word are recognised anyway, so nothing real is
    lost: Rust's <code>thread '…' panicked at</code>, and the JVM's <code>Error: Could not find or
    load main class</code>.</li>
  <li><strong>Stack frames are links.</strong> A frame in a class this project declares
    (<code>at com.acme.Order.total(Order.java:118)</code>) opens that file at that line.
    A frame in the <strong>JDK or a dependency</strong> opens too, in that class's source view
    — the real <code>.java</code> from the JDK's <code>src.zip</code> or from a downloaded
    <code>-sources.jar</code> when there is one, otherwise the stub decompiled from its
    bytecode, where the tab offers <em>Download sources</em>. Those frames are drawn muted:
    a Spring stack trace is forty lines of framework around three lines of yours, and telling
    them apart at a glance is worth more than the click. <em>URLs</em> open in the browser and
    <em>paths</em> (with the <code>:42</code> a compiler appends) open in the editor.</li>
  <li>A frame naming something <strong>made at runtime</strong> — a lambda's carrier, a CGLIB
    or JDK proxy, a generated reflection accessor — is marked but not a link. No source for it
    exists anywhere, and a link that always fails teaches you not to click any of them.</li>
  <li><strong>A stack trace stays part of its error.</strong> The frames under an
    <code>ERROR</code> say nothing about their own severity, so they inherit it — including
    across <code>Caused by:</code> — and an ordinary line ends the inheritance. It also means
    a routine <code>INFO</code> that a program wrote to <em>standard error</em> — Tomcat and
    <code>java.util.logging</code> both do — is no longer painted red along with everything
    else on that stream.</li>
  <li><strong>Colours are honoured.</strong> A program that writes ANSI colour codes is coloured
    rather than showing the escape codes as text, and its own colour wins over the one inferred
    from the text — it knows something about its output that a scanner does not. Everything else
    a terminal would act on — cursor movement, erase-line — is discarded, because this is a
    transcript of what was printed and a transcript has no cursor to move. For the real thing
    there is a real terminal at <kbd>Alt</kbd> + <kbd>F12</kbd>.</li>
  <li>The console <strong>follows new output only while you are at the bottom</strong>. Scroll up
    to read something and it stays where you put it.</li>
  <li><strong>Long lines scroll sideways</strong> rather than wrapping, the way a terminal's do,
    so one line is always one row. A line longer than four thousand characters — the
    <code>toString</code> of a loaded collection, a response body logged whole — is cut, and says
    how much was left off. Ten thousand lines are kept per tab; beyond that the oldest go.</li>
  <li><strong>One tab per run.</strong> A run is something that happened, and comparing this one
    against the last is most of what a console is for — so a launch opens a tab rather than
    wiping the previous transcript. The eight most recent are kept; ⟳ repeats <em>the tab you
    are looking at</em>, into a new tab; the 🗑 closes the finished ones and leaves anything
    still running. Closing a tab whose program is still running stops it.</li>
</ul>
<p>
  One program runs at a time: launching while one is going is refused, so the other tabs are
  history rather than a set of live consoles.
</p>
<h2>Which Java runs it</h2>
<p>
  The same one the project is analysed with — the level from <code>maven.compiler.release</code>,
  <code>source</code>/<code>target</code>, <code>&lt;java.version&gt;</code> or the compiler
  plugin, resolved to an installed JDK and handed to Maven as <code>JAVA_HOME</code>. When no JDK
  of that level is installed, the build inherits whatever <code>JAVA_HOME</code> your environment
  sets rather than being pointed at a different one. See <em>Projects &amp; capabilities</em>.
</p>
<h2>Things worth knowing</h2>
<ul>
  <li>A launch <strong>builds first</strong> and stops if the compile fails — the console says so
    and the Build panel has the details. That build is <code>mvn compile</code>: no
    <code>clean</code> (so a class whose source you deleted stays in <code>target/classes</code>
    until you clean by hand) and no <code>package</code> (no jar or war is produced — the run
    starts from <code>target/classes</code> and the dependency jars). Resources are copied, so
    an edited <code>application.yml</code> is picked up.</li>
  <li><strong>Nothing changed means nothing runs.</strong> Before invoking Maven, Bennu stamps
    the modules' <code>src/main/java</code> and <code>src/main/resources</code> — sizes and
    modification times, no file opened — and compares it against the last successful compile.
    Unchanged, it says <em>Up to date</em> and launches immediately. That check is the
    difference an IDE has over a build tool: Maven's floor is seconds <em>with nothing to
    do</em>, because every invocation re-reads every pom, re-resolves every module's plugins
    and re-runs their up-to-date checks before concluding there was nothing to compile. When
    it does have to compile, it compiles only the run's module and the ones it is built from,
    not the whole reactor. The stamp is per session and is dropped when the project is
    re-indexed, so a <code>mvn clean</code> run in a terminal cannot leave it lying.</li>
  <li>A <strong>Spring Boot</strong> configuration runs the class directly, the way an IDE does —
    not through <code>spring-boot:run</code>. Devtools and the plugin's own resource handling are
    therefore not in play.</li>
  <li>Only <strong>one</strong> build, validation or test run at a time: they all touch
    <code>target/</code>, and two Maven processes on one tree fight over it. A launched program is
    not part of that lock — it is already running.</li>
  <li>A launched program is not part of the build lock — it is already running — but only one
    of them at a time: launching while one is going is refused.</li>
</ul>
