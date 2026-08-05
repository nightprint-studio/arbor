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
</ul>
<p>
  On a project with exactly one entry point you need none of this: press ▷ and Bennu finds it,
  makes a configuration for it — a Spring Boot one if that class is a Boot application — and
  runs it. With several, it opens the editor and asks, which is a real question.
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

<h2>Debugging</h2>
<p>
  <kbd>Shift</kbd> + <kbd>F9</kbd>, or the 🐞 beside ▶, launches the same configuration with a
  debugger attached. Click in the left margin — outside the line numbers — to set a breakpoint;
  <kbd>Ctrl</kbd> + <kbd>F8</kbd> sets one on the caret's line. Breakpoints belong to the
  <em>project</em>, not to a session: they are kept in <code>.arbor/bennu/config.toml</code> beside the
  run configurations, so they are still there tomorrow, and a launch installs whatever is set.
</p>

<h3>Where one can go</h3>
<p>
  A breakpoint is a position in <em>bytecode</em>, so only a line that compiles to some has one.
  A package statement, an annotation, a class or method signature, a field declaration with
  nothing to run, a comment or a lone brace all compile to nothing — and the margin simply does
  not offer a breakpoint there. The faint dot under the pointer appears on the lines that can
  take one, and nowhere else: there is nothing to press rather than a press that quietly puts
  the breakpoint three lines further down.
</p>
<p>
  A field <em>with</em> an initializer is offered, because its initializer runs. So is any
  statement. A breakpoint already set on a line stays clickable even if an edit turns that line
  into something that no longer qualifies — otherwise you could not remove it.
</p>

<h3>The dot in the margin</h3>
<ul>
  <li><strong>Solid</strong> — the VM accepted it and the program will stop there.</li>
  <li><strong>Hollow</strong> — the class has not been loaded yet. This resolves itself the moment
    the program touches it; nothing to do.</li>
  <li><strong>Outlined in grey</strong> — disabled. Right-click a breakpoint to disable it rather
    than delete one you will want back in ten minutes.</li>
</ul>
<p>
  <kbd>Ctrl</kbd> + <kbd>Shift</kbd> + <kbd>F8</kbd> opens the <strong>breakpoint list</strong>:
  every one in the project, grouped by file, with a switch to disable and a bin to remove — and
  the place to add an <strong>exception breakpoint</strong>, which stops where a throwable is
  <em>thrown</em> rather than where it is caught. That is the only way to see the state that
  produced it, and it has no line to click, so the gutter cannot offer it.
</p>
<p>
  The ⊘ in the debugger's controls <strong>mutes</strong> every breakpoint at once: they stay
  set and stay listed, but the VM is not asked about them, so the program runs to its end at
  full speed. For getting past the twelve you will want back in a minute. Muting lasts for the
  session — a debugger that silently ignored your breakpoints tomorrow would be a trap.
</p>
<p>
  Breakpoints follow the lines they are on as you edit above them. If one still ends up on a line
  the compiler produced no code for, it binds to the statement underneath and the tooltip says
  which line it really stopped on.
</p>

<h3>It all happens in the Run console</h3>
<p>
  There is no separate Debug window. Debugging is not a different activity from running — it is
  the same launch with more to look at — so the <strong>Run</strong> panel
  (<kbd>Alt</kbd> + <kbd>R</kbd>) grows what the moment needs and nothing when nothing is being
  debugged. While a session is attached, the transport controls sit on the left of the status
  row. While the program is <em>stopped</em>, two columns open to the left of the transcript:
  the <strong>frames</strong> that got here, and the <strong>variables</strong> in the selected
  one with your <strong>watches</strong> underneath.
</p>
<p>
  <strong>The editor follows the debugger.</strong> Every stop and every step opens the current
  frame's file at its line and scrolls to it — stepping into a method changes the tab for you.
  Clicking a frame does the same, and re-reads the variables and watches against it. The line the
  program is stopped on is banded across its full width with a bar down its left edge.
</p>
<p>
  Both columns are <strong>resizable</strong> — drag the divider — and each collapses to a
  labelled strip from the ⇤ in its header, so a session where you only want the stack, or only
  the values, gives the transcript the rest. Where you left them is remembered.
</p>
<p>
  Frames in libraries and the JDK are drawn muted, and clicking one opens its source from a
  <code>-sources.jar</code> or a decompiled stub, the same way a stack trace in the console
  does. A run of consecutive library frames <strong>folds into one row</strong> you can expand —
  a stop inside a framework is forty frames of Spring and reflection around the three that are
  yours. The ⊟ in the Frames header turns the folding off; the choice is remembered.
</p>
<p>
  Stepping is <kbd>F8</kbd> over, <kbd>F7</kbd> into, <kbd>Shift</kbd> + <kbd>F8</kbd> out, and
  <kbd>F9</kbd> resumes.
</p>
<p>
  A step <strong>passes straight through</strong> the JDK, Spring's AOP machinery, Reactor and
  the logging façades, rather than stopping inside <code>ArrayList.add</code> or walking
  <code>ReflectiveMethodInvocation.proceed</code> a dozen times. And when it lands in a
  <strong>proxy</strong> — a Spring CGLIB or Hibernate stand-in, whose methods exist only at
  runtime — it keeps going rather than stopping in a class whose source has never been written.
  Stepping into <code>service.place(order)</code> arrives at <code>place</code>.
</p>
<p>
  Which packages those are is a judgement about whose code you are debugging, so it is yours to
  change: <strong>Settings → Debugger</strong> lists them, and removing
  <code>org.springframework.*</code> is how you step into Spring itself. A pattern may carry a
  <code>*</code> at one end only — the VM refuses anything else, and one bad entry would stop
  stepping working at all, so it is checked as you type. Changes apply to the next launch.
</p>
<p>
  When the program stops, <strong>Bennu comes to the front</strong> and opens the console. A
  breakpoint fires because of something happening in another window — a browser, a terminal, a
  request from elsewhere — and the editor is the only place the answer is. Stepping does not
  raise it, since a step is something you did here.
</p>

<h3>Watches</h3>
<p>
  A watch is a <strong>path</strong>: <code>order</code>, <code>order.customer.name</code>,
  <code>items[2]</code> — a variable, then fields and array subscripts. It is not an expression
  language, and anything else is refused by name rather than quietly evaluated as something
  adjacent. Watches are persisted with the breakpoints.
</p>

<h3>Things worth knowing</h3>
<ul>
  <li><strong>Objects show their fields, not their <code>toString()</code>.</strong> An object
    reads as <code>Order@1f3c</code> and expands to its real fields, superclasses included.
    Calling <code>toString()</code> would read better and would mean running application code
    inside a paused program — which can block on a lock the stopped thread holds, change state,
    or throw.</li>
  <li><strong>Variable names need <code>-g</code>.</strong> Maven compiles with full debug
    information by default, so this is normally there. A class compiled without it can still hold
    breakpoints — the line table is separate — but the panel will only be able to show
    <code>this</code>.</li>
  <li><strong>The launch does not begin frozen.</strong> The program starts and runs, which means
    a breakpoint in start-up code may be passed before the debugger is listening. When you need
    one there, tick <em>Suspend the VM until the debugger has attached</em> in the run
    configuration — every debug launch of it then begins stopped, waiting for Resume.</li>
  <li><strong>Detach is not Stop.</strong> Detaching leaves the program running with no debugger
    attached; the Run console's ■ is what ends it. A suspended VM holds its locks and its port,
    so a session left paused looks exactly like a hang — the rail's Debug dot turns amber to say
    so.</li>
  <li>JUnit configurations cannot be debugged yet: Maven forks its own JVM for Surefire, which is
    a different launch path rather than a flag.</li>
</ul>
