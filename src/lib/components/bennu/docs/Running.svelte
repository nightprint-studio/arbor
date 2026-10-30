<script lang="ts">
  /**
   * Building and running: the build, the ▶ in the gutter, run configurations and their categories, multi-module classpaths,
   * the Run console, which Java runs, and what is worth knowing about the build in front of a launch.
   */
  import Callout from '$lib/components/shared/ui/Callout.svelte';
</script>

<span class="eyebrow">Build, run &amp; test</span>
<h1>Building &amp; running</h1>

<p class="doc-lead">
  Compile the project, launch its <code>main</code>, and watch — and answer — its output without leaving the window.
</p>

<h2>Building</h2>
<table>
  <thead><tr><th>Project</th><th><kbd>Ctrl</kbd> + <kbd>F9</kbd> runs</th></tr></thead>
  <tbody>
    <tr><td>Maven</td><td><code>mvn compile</code>, under the JDK the project declares</td></tr>
    <tr><td>Cargo</td><td><code>cargo check</code> — the button is for diagnostics, and <code>check</code> reaches them without linking</td></tr>
  </tbody>
</table>
<p>
  The split button beside it offers <strong>Validate (no compile)</strong>: the editor's own analysis over every file, seconds instead of a full build, answering only what an editor
  can answer — see <strong>Validation &amp; problems</strong>.
</p>
<p>
  Output lands in the <strong>Build</strong> panel (<kbd>Alt</kbd> + <kbd>0</kbd>): the parsed errors first, each clickable to its line, then the log — interpreted as the Run console's
  is, so Maven's <code>[ERROR]</code>s stand out and paths and stack frames are links. A clean Java build indexes <code>target/classes</code> again, so completion sees what you just
  compiled.
</p>

<h2>The ▶ in the gutter</h2>
<table>
  <thead><tr><th>File</th><th>Where the ▶ is</th><th>What it runs</th></tr></thead>
  <tbody>
    <tr><td>Java</td><td>Beside a <code>main</code>, in the framework marks' column</td><td><strong>Run</strong>, <strong>Debug</strong> or <em>Edit configurations…</em> — running launches the class with no arguments, no VM flags, the <code>runtime</code> classpath</td></tr>
    <tr><td>Rust</td><td>A code lens above every <code>fn main</code> and <code>#[test]</code>, from rust-analyzer</td><td>Exactly what the server says — the package, the binary, or one test with <code>--exact</code>; <em>Debug</em> builds and launches the binary under the debugger</td></tr>
    <tr><td>Scripts — <code>.sh</code>, <code>.bat</code>/<code>.cmd</code>, <code>.ps1</code></td><td>On the first line</td><td>The script, after saving the buffer — you just fixed the line, and the copy on disk would not have it</td></tr>
  </tbody>
</table>
<ul>
  <li>Nothing is saved behind your back: pressing ▶ eleven times leaves no configurations. A run that needs arguments or flags wants a configuration — the third entry.</li>
  <li>The Java ▶ appears only for a class the project's <strong>entry-point scan</strong> knows, so what launches is a class the compiler agrees exists. Its line is read from the buffer,
    so a <code>main</code> just typed gets its arrow at once.</li>
</ul>
<Callout variant="info" title="What a script needs on this machine">
  The refusal says so instead of the arrow going grey. A <code>.bat</code> is <code>cmd.exe</code> syntax, Windows only. A <code>.sh</code> on Windows needs <strong>Git Bash</strong>,
  looked for under Program Files and <code>%LOCALAPPDATA%\Programs\Git</code> — not the <code>bash.exe</code> in <code>System32</code>, the WSL launcher, whose scripts see another
  filesystem. A <code>.ps1</code> outside Windows needs PowerShell 7, <code>pwsh</code>.
</Callout>

<h2>Run configurations</h2>
<p>
  A run configuration is a named launch target, kept in <code>&lt;project&gt;/.arbor/bennu/config.toml</code> — per project, surviving a restart, and yours to commit or ignore.
</p>
<p>
  The <strong>selector in the title bar</strong>, left of ▷, says which one those buttons start, and opens the list, grouped by category. Picking one makes it
  <strong>active</strong> — it does not launch it. Choosing what ▷ means and pressing ▷ are separate acts.
</p>

<h3>Categories</h3>
<div class="feature-grid">
  <div class="feature-card">
    <div class="fc-eyebrow">Java</div>
    <div class="fc-title">Application</div>
    <div class="fc-desc">A module, a <code>main</code> class, program and VM arguments, a working directory, environment variables. <em>Choose</em> lists the module's classes with <code>public static void main(String[])</code>, and filling one fills the module.</div>
  </div>
  <div class="feature-card">
    <div class="fc-eyebrow">Java, with Spring</div>
    <div class="fc-title">Spring Boot</div>
    <div class="fc-desc">The same launch plus <strong>active profiles</strong>, as <code>-Dspring.profiles.active=…</code>; <em>Detected</em> lists the project's <code>application-«profile»</code> files, and the field stays free text. <strong>The main class is optional</strong>: a Boot module has one <code>@SpringBootApplication</code>.</div>
  </div>
  <div class="feature-card">
    <div class="fc-eyebrow">Java</div>
    <div class="fc-title">JUnit</div>
    <div class="fc-desc">A test scope — the project, a module or a class — run through the test runner, into its own tab with the test tree beside the output.</div>
  </div>
  <div class="feature-card">
    <div class="fc-eyebrow">Rust</div>
    <div class="fc-title">Cargo</div>
    <div class="fc-desc">A subcommand: crate, command, target, features, profile — pickers, since the workspace knows them — and <strong>Cargo arguments</strong> before the <code>--</code>, <strong>Program arguments</strong> after.</div>
  </div>
  <div class="feature-card">
    <div class="fc-eyebrow">Every project</div>
    <div class="fc-title">Script</div>
    <div class="fc-desc">A <code>.sh</code>, <code>.bat</code>/<code>.cmd</code> or <code>.ps1</code>, picked not typed, with arguments, a working directory and environment variables. <strong>Run with</strong> chooses the shell from those that can interpret the file — <em>Git Bash</em>, <em>zsh</em>, <em>sh</em> for a <code>.sh</code>; <em>PowerShell 7</em> or <em>Windows PowerShell</em> for a <code>.ps1</code> — and <em>Auto</em> picks by extension. A shell this machine lacks is listed, greyed, with the reason. Whether the script runs <em>here</em> is answered at launch. No Debug: a shell script's debugger is <code>set -x</code>.</div>
  </div>
</div>
<h3>Environment</h3>
<p>
  Every category has an <strong>Environment</strong> section, folded while it is empty. Variables are edited as a <strong>table</strong> or as <strong>text</strong> — one <code>KEY=value</code> per line, so a block pasted
  from a README fills the list. A value whose name suggests a credential (<code>PASSWORD</code>, <code>TOKEN</code>, <code>SECRET</code>…) is hidden until you press the eye.
</p>
<p>
  The <strong>.env file</strong> field points at a file that is <strong>read every time the configuration runs</strong> — nothing is copied into the configuration, so editing the file is enough and a secret kept in an ignored
  <code>.env</code> never reaches the committed run configuration. The variables typed in the section win over the file's. A relative path is taken from the project root; an unreadable file stops the launch with a message in the Run console.
</p>
<p>
  In the Spring Boot form the less used parts — launch arguments, classpath and debugger — are folded sections too; a folded header shows what is in it.
</p>

<p>
  Each category is offered only where it applies — JVM ones on a Java project, Cargo on a Cargo one, Spring Boot where there is Spring. On a project with exactly one entry point you need
  none of this: ▷ finds it, makes a configuration — a Spring Boot one for a Boot application, a Cargo one for a workspace's single binary — and runs it. With several, it asks.
</p>
<p>
  A Cargo configuration has <strong>no build step in front of it</strong>: the command <em>is</em> the build, and prefixing one would compile the workspace twice. Its debugger is a
  debug adapter rather than JDWP — see <strong>Debugging</strong>.
</p>

<h3>Multi-module projects</h3>
<p>
  The <strong>module</strong> is a configuration's first field because it decides the classpath — a reactor's root usually compiles nothing, and a run without a module launches against
  a directory that does not exist. The classpath is the module's <code>target/classes</code>, then the <code>target/classes</code> of the modules it depends on, then the
  dependencies: your inner loop is compile-and-run without <code>mvn install</code> or <code>package</code>, so a call across modules finds the sibling's classes where they are.
</p>
<ul>
  <li><strong>Resolved from inside the reactor.</strong> The dependencies are asked of Maven as <code>-pl «module» -am</code> at the project root, so a sibling module arrives as its
    <code>target/classes</code> — never as a jar a past <code>mvn install</code> left in <code>~/.m2</code>, and a sibling that was never installed does not break the resolve.
    A sibling jar that still turns up is replaced by that module's classes.</li>
  <li><strong>Only the modules it depends on.</strong> A sibling nothing depends on stays off the classpath, with its <code>application.yml</code>, its beans and its
    <code>META-INF</code> files.</li>
  <li><strong>Refreshed when any pom changes</strong> — the module's, a parent's or a sibling's, since a sibling's new dependency is this module's too — and when an entry has
    disappeared from disk.</li>
  <li><strong>A resolve that fails stops the launch</strong>, with Maven's reason in the console. Jars Maven named and could not find offline are listed at the top of the run.</li>
</ul>
<Callout variant="warning" title="The runtime scope, on purpose">
  Dependencies are resolved at the <strong>runtime</strong> scope — what <code>mvn spring-boot:run</code> and a packaged application see — narrower than the editor, which resolves every
  scope to edit tests. Launching with the wider one hands the JVM libraries Maven never supplies: a <code>@ConditionalOnClass</code> on a test-scoped library then fires here and nowhere
  else, and the application refuses to start in the IDE while Maven is happy.
</Callout>
<p>
  <strong>Classpath</strong> in the configuration changes it — Compile, Test, or every scope — because a launcher wanting a test-scoped H2 or a provided servlet API is legitimate. The
  first launch of a configuration resolves its classpath through Maven; later ones are instant until a pom changes. The resolve starts offline, from <code>~/.m2</code>. A jar Maven refuses offline although it is on disk — "has not been downloaded from it before", as after IntelliJ filled the repository — is accepted on a second offline try that ignores where it came from. If a jar is
  still missing and <strong>Settings → Java → Allow Maven to use the network</strong> (the Maven panel's <em>Offline</em> toggle) is on, the launch goes online once to fetch it — the console says so — instead of starting a JVM that dies
  on a missing class. With that switch off the launch stays offline and says which entries are absent. The working directory defaults to the module's, and both the
  editor's list and the title-bar selector show a configuration's module.
</p>
<p>
  Entirely from the keyboard: <em>Edit run configuration…</em> in the palette opens the editor with the list focused; <kbd>↑</kbd>/<kbd>↓</kbd> move, ● makes one active,
  <kbd>Enter</kbd> runs it, <kbd>Ctrl</kbd> + <kbd>Enter</kbd> closes. Every edit is written as you type.
</p>

<h2>The Run console</h2>
<p>
  <kbd>Shift</kbd> + <kbd>F10</kbd> builds and launches, and <strong>Run</strong> (<kbd>Alt</kbd> + <kbd>R</kbd>) opens on the program's own output — a separate window from Build,
  because a build log is finished when you read it while a program's output is live: watched, typed into, stopped.
</p>

<h3>The run itself</h3>
<dl class="meta-grid">
  <dt>Its state</dt>
  <dd>A coloured pill at the left of the status row says what the run is doing: green <em>Running</em>, red <em>Debugging</em> while a debugger is attached, amber <em>Paused</em> at a breakpoint, and once it is over a green <em>Finished</em> or a red
    <em>Exited with code …</em>, with its duration. The same state is the glyph on the run's tab, so a strip of several runs shows which is going, which is under the debugger and which failed without opening any of them.</dd>
  <dt>The command that ran</dt>
  <dd>On the same row, after the state: one line, cut at the end of the panel, with the whole of it in the tooltip and a copy button — selectable too. the resolved <code>java</code>, VM arguments and class, quoted to paste into a terminal, the classpath summarised as a count. A real classpath does not fit a command line (Windows caps one at 32 767 characters), so it goes in a JDK <strong>argument file</strong>, <code>@«module»/target/bennu-run.args</code> — still exactly what you can paste. A Java 8, with no argument files, gets it through <code>CLASSPATH</code>.</dd>
  <dt>Typing back</dt>
  <dd>While a program runs, <code>&gt; Send input</code> opens a line writing to its standard input, so a prompt can be answered rather than look like a hang; what you send is echoed with <code>&gt;</code>, and <kbd>Esc</kbd> puts it away. It starts closed on every run — Bennu cannot tell from outside whether a program is waiting.</dd>
  <dt>Stop</dt>
  <dd>Stops the whole process tree, not just the launched process.</dd>
  <dt>The verdict</dt>
  <dd>The exit code and how long the run took.</dd>
</dl>

<h3>Reading the output</h3>
<ul>
  <li><strong>Every line is interpreted</strong> — level, timestamp, thread, logger, exception, URL, path — and coloured, so the line that matters is findable. Levels count in upper case
    (<code>ERROR</code>, <code>[WARN]</code>, <code>SEVERE</code>): painting the word <em>Error</em> in a sentence red would empty the colours of meaning.</li>
  <li><strong>What a line says beats where it came from.</strong> <code>rustc</code>, <code>cargo</code>, <code>javac</code> and <code>gcc</code> write warnings to standard error, so a
    <code>warning:</code> is amber, an <code>error:</code> red, rustc's <code>note:</code> and <code>help:</code> quiet, and the source excerpt under a diagnostic stays part of it.</li>
  <li><strong>Red means the line said error.</strong> A line nobody interprets is neutral whichever pipe it came down — <code>cargo</code> writes its whole log to standard error. The two
    failures with no level word are recognised anyway: Rust's <code>thread '…' panicked at</code>, and the JVM's <code>Error: Could not find or load main class</code>.</li>
  <li><strong>A stack trace stays part of its error</strong>: frames under an <code>ERROR</code> inherit it, across <code>Caused by:</code>, until an ordinary line. So an
    <code>INFO</code> Tomcat or <code>java.util.logging</code> wrote to standard error is no longer red.</li>
  <li><strong>Colours are honoured</strong>: a program's ANSI colours win over inferred ones. Cursor movement and erase-line are discarded — this is a transcript. The real terminal is
    <kbd>Alt</kbd> + <kbd>F12</kbd>.</li>
</ul>

<h3>Links</h3>
<ul>
  <li>A frame in a class this project declares — <code>at com.acme.Order.total(Order.java:118)</code> — opens that file at that line.</li>
  <li>A frame in the <strong>JDK or a dependency</strong> opens its source view: the real <code>.java</code> from <code>src.zip</code> or a <code>-sources.jar</code>, otherwise the
    decompiled stub, where the tab offers <em>Download sources</em>. Those frames are muted — a Spring trace is forty framework lines around three of yours.</li>
  <li>A frame of something <strong>made at runtime</strong> — a lambda carrier, a CGLIB or JDK proxy, a generated accessor — is marked but not a link: no source exists anywhere.</li>
  <li><em>URLs</em> open in the browser, and <em>paths</em>, with the <code>:42</code> a compiler appends, in the editor.</li>
  <li>A <strong>class named in a message</strong> — <code>com.acme.order.OrderService</code>, an exception, a logger — opens its file when this project declares it. Write the
    method too and the editor lands on it: <code>com.acme.Order#total</code>, <code>Order::total</code> or <code>com.acme.Order.total</code>. A name no class of the project answers to is
    coloured but not a link, so a click never opens the wrong class.</li>
</ul>

<h3>The project's log format</h3>
<p>
  When the project says how its logs are written, the console reads it instead of guessing. Bennu looks in <code>logback-spring.xml</code>, <code>logback.xml</code>,
  <code>log4j2-spring.xml</code>, <code>log4j2.xml</code> and the <code>logging.pattern.console</code> of <code>application.properties</code> / <code>.yml</code>, in the project and its
  modules' <code>src/main/resources</code>. For a pattern like
</p>
<pre><code>{'%d{yyyy-MM-dd HH:mm:ss} | %-5level | %logger{1} | MDC:[tenant=%X{tenant.id},trace_id=%X{Trace-Id}] | %msg%n'}</code></pre>
<p>
  the date, the level and the thread are known by position, the <strong>logger</strong> column is the class (a bare <code>OrderService</code>, or abbreviated like
  <code>c.a.o.OrderService</code>, opens its file when exactly one class fits), and every MDC entry is marked — the <strong>key</strong> quiet, the <strong>value</strong> it names in colour. An <code>MDC:[tenant=acme,trace_id=…]</code> block is recognised by its shape even when no
  configuration is found, as is any bracketed group of two or more <code>key=value</code> pairs. The message after the prefix is read as before, so a URL or a class name
  inside it is still a link. Lines that are not in the layout — stack frames, other libraries' output — keep the ordinary rules.
</p>
<p>
  A pattern the console cannot model (<code>%replace</code>, an unresolved <code>{'${property}'}</code>, Spring Boot's built-in default) is not used, and the output is read by shape alone.
  The configuration is read when a run starts.
</p>

<h3>Tabs</h3>
<ul>
  <li><strong>Each tab says which project it came from</strong> in a workspace with more than one — a chip after the label, since run
    configurations are named after what they launch and three projects with a <code>Main</code> produce three tabs called <code>Main</code>. The tab's
    tooltip names the project whatever the workspace holds.</li>
  <li><strong>Tabs can be dragged</strong> to put them in the order you want, like the editor's; the Tests tab, when there is one, stays first. They keep their full width up to the buttons at the right of the header.</li>
  <li><strong>One tab per run</strong>, so this run can be compared with the last. The eight most recent are kept; ⟳ repeats <em>the tab you are looking at</em>, into a new tab; 🗑 closes
    the finished ones. Past the width of the strip the rest are in the <strong>⌄</strong> menu at its end. Closing a tab whose program is still going stops it —
    and says so first, naming what is about to stop.</li>
  <li>The console <strong>follows new output only while you are at the bottom</strong>; scroll up and it stays put.</li>
  <li><strong>Long lines scroll sideways</strong>, one line one row. Past four thousand characters a line is cut and says how much; ten thousand lines are kept per tab.</li>
  <li>Several programs run at once — a server and its client — each with its tab, stdin and Stop, and a ▷ in the strip on live tabs. <strong>Stop and the input line act on the tab in
    front</strong>; looking at a finished one with several running, Stop waits for you to pick.</li>
</ul>

<h2>Which Java runs it</h2>
<p>
  The one the project is analysed with — the level from <code>maven.compiler.release</code>, <code>source</code>/<code>target</code>, <code>&lt;java.version&gt;</code> or the compiler
  plugin, resolved to an installed JDK and handed to Maven as <code>JAVA_HOME</code>. With no JDK of that level installed, the build inherits your environment's <code>JAVA_HOME</code>.
  See <strong>The JDK</strong>.
</p>

<h2>Things worth knowing</h2>
<dl class="meta-grid">
  <dt>A launch builds first</dt>
  <dd>And stops if the compile fails — the console says so, the Build panel has details. That build is <code>mvn compile -pl «module» -am</code>: the run's module and the modules it is built from, and no <code>package</code> — the run starts from <code>target/classes</code> and the jars. Resources are copied, so an edited <code>application.yml</code> is picked up, and a deleted one is removed from <code>target/classes</code>. Without Maven there is no launch: the <code>javac</code> fallback only reports errors, and does not rebuild the classes a run uses.</dd>
  <dt>Nothing changed, nothing runs</dt>
  <dd>Before Maven, Bennu compares each module in the build — its <code>src/main</code> and its poms, sizes and times, no file opened — with how it looked at its last successful compile. Unchanged, it says <em>Up to date</em> and launches at once: Maven's floor is seconds <em>with nothing to do</em>. A build of one module never counts for a module outside it, and a module whose <code>target/classes</code> is missing is compiled however quiet the rest has been. The record is kept between sessions, so the first launch after a restart is as fast as the next one — but it is never taken on faith: the files are compared again, and a module's record is used only while its <code>target/classes</code> still holds the newest class that compile left. A <code>mvn clean</code>, or a build run from a terminal, makes it void and the launch compiles. The resolved run classpath is kept the same way, and used only while the poms are unchanged and every entry is still on disk. <em>Rebuild index</em> drops both.</dd>
  <dt>Projects build independently</dt>
  <dd>Two different projects share no <code>target/</code> and no index, so one compiling does not hold up the other: launch a second project while the first is still compiling and it starts at once. Two builds of the <em>same</em> project do line up — a launch pressed while that project is compiling or being validated waits for it (the console says so) instead of being dropped.</dd>
  <dt>No <code>mvn clean</code> needed</dt>
  <dd>Maven's incremental compile leaves three kinds of stale output, and Bennu removes them before compiling, rebuilding that module from scratch — its <code>.class</code> files, <code>target/generated-sources/annotations</code> and the compiler's status — and saying why in the Build log:
    <ul>
      <li><strong>A module it depends on changed</strong> since it was compiled — including when another launch has already recompiled that module, which Maven cannot see, and which otherwise ends in <code>NoSuchMethodError</code>. With no record from this session, a module whose classes are older than a dependency's counts too.</li>
      <li><strong>A source was deleted or renamed</strong>, whose class would otherwise stay behind for Spring to scan.</li>
      <li><strong>Its pom or a parent pom changed.</strong></li>
    </ul>
    A module rebuilt this way takes the modules built from it along. And a compile that fails inside <code>target/generated-sources</code>, or on <code>duplicate class</code> — MapStruct or Lombok output left from an earlier compile — is rebuilt from scratch and compiled once more on its own.</dd>
  <dt>Spring Boot runs the class directly</dt>
  <dd>As an IDE does, not through <code>spring-boot:run</code> — so devtools and the plugin's resource handling are not in play.</dd>
  <dt>One build at a time</dt>
  <dd>A build, a validation or a test run: they all touch <code>target/</code>, and two Maven processes on one tree fight over it. Launched programs are not part of that lock — a launch is refused only while something compiles.</dd>
</dl>
