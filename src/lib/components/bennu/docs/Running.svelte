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
    <div class="fc-desc">A <code>.sh</code>, <code>.bat</code>/<code>.cmd</code> or <code>.ps1</code>, picked not typed, with arguments, a working directory and environment variables. Whether it runs <em>here</em> is answered at launch. No Debug: a shell script's debugger is <code>set -x</code>.</div>
  </div>
</div>
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
  a directory that does not exist. The classpath is the module's <code>target/classes</code>, then every other module's, then the dependencies: your inner loop is compile-and-run
  without <code>mvn install</code>, so a call across modules must find the sibling's classes where they are.
</p>
<Callout variant="warning" title="The runtime scope, on purpose">
  Dependencies are resolved at the <strong>runtime</strong> scope — what <code>mvn spring-boot:run</code> and a packaged application see — narrower than the editor, which resolves every
  scope to edit tests. Launching with the wider one hands the JVM libraries Maven never supplies: a <code>@ConditionalOnClass</code> on a test-scoped library then fires here and nowhere
  else, and the application refuses to start in the IDE while Maven is happy.
</Callout>
<p>
  <strong>Classpath</strong> in the configuration changes it — Compile, Test, or every scope — because a launcher wanting a test-scoped H2 or a provided servlet API is legitimate. The
  first launch of a configuration resolves its classpath through Maven; later ones are instant until the pom changes. The working directory defaults to the module's, and both the
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
  <dt>The command that ran</dt>
  <dd>The first line: the resolved <code>java</code>, VM arguments and class, quoted to paste into a terminal, the classpath summarised as a count. A real classpath does not fit a command line (Windows caps one at 32 767 characters), so it goes in a JDK <strong>argument file</strong>, <code>@«module»/target/bennu-run.args</code> — still exactly what you can paste. A Java 8, with no argument files, gets it through <code>CLASSPATH</code>.</dd>
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
</ul>

<h3>Tabs</h3>
<ul>
  <li><strong>One tab per run</strong>, so this run can be compared with the last. The eight most recent are kept; ⟳ repeats <em>the tab you are looking at</em>, into a new tab; 🗑 closes
    the finished ones. Closing a running tab stops its program.</li>
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
  <dd>And stops if the compile fails — the console says so, the Build panel has details. That build is <code>mvn compile</code>: no <code>clean</code>, so a class whose source you deleted stays in <code>target/classes</code>; no <code>package</code>, so no jar or war — the run starts from <code>target/classes</code> and the jars. Resources are copied, so an edited <code>application.yml</code> is picked up.</dd>
  <dt>Nothing changed, nothing runs</dt>
  <dd>Before Maven, Bennu stamps the modules' <code>src/main/java</code> and <code>src/main/resources</code> — sizes and times, no file opened — against the last successful compile. Unchanged, it says <em>Up to date</em> and launches at once: Maven's floor is seconds <em>with nothing to do</em>. When it must compile, only the run's module and the ones it is built from. The stamp is per session and dropped on reindex, so a terminal <code>mvn clean</code> cannot leave it lying.</dd>
  <dt>Spring Boot runs the class directly</dt>
  <dd>As an IDE does, not through <code>spring-boot:run</code> — so devtools and the plugin's resource handling are not in play.</dd>
  <dt>One build at a time</dt>
  <dd>A build, a validation or a test run: they all touch <code>target/</code>, and two Maven processes on one tree fight over it. Launched programs are not part of that lock — a launch is refused only while something compiles.</dd>
</dl>
