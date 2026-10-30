<script lang="ts">
  /**
   * The profiler: recording a JVM, Node or native program, reading the recording, and the numbers
   * in the editor. What each language needs and what is deliberately not claimed.
   */
  import Callout from '$lib/components/shared/ui/Callout.svelte';
</script>

<span class="eyebrow">Build, run &amp; test</span>
<h1>Profiler</h1>

<p class="doc-lead">
  Where does the time go, and what allocates? The profiler records a run, folds the samples into a call tree and shows it as a flame graph, a table of hot methods,
  allocations, garbage-collection pauses and lock waits. The same numbers are written <em>in the editor</em>, on the method they belong to.
</p>

<h2>Opening it</h2>
<p>
  <kbd>Ctrl</kbd> + <kbd>Shift</kbd> + <kbd>F9</kbd>, the flame button on the right rail, or <strong>Toggle Profiler</strong> in the command palette (a finished recording opens it by itself). Recordings are kept per
  project, newest first (the last twenty), and survive restarting Bennu.
</p>

<h2>Recording</h2>
<p>
  The gauge button beside the bug (<kbd>Shift</kbd> + <kbd>F11</kbd>, <strong>Profile</strong> in the palette and the ⋮ menu) records the run configuration selected in the title bar: an Application or Spring Boot one under
  JDK Flight Recorder, a Cargo one that runs a binary (<code>cargo run</code>) under samply. While it records, the button is <strong>Stop</strong>. A script, a JUnit run or a <code>cargo check</code> configuration has no
  program to look inside, so the button is dimmed — still clickable, because its tooltip and a toast say why. <strong>Profile a command or running JVM…</strong> (the ⋮ menu, the palette, or <strong>Profile…</strong> in
  the panel) opens a dialog for everything else: a Node or native command, a JVM that is already running, and the samply tool. The panel itself only reads recordings and shows a recording in progress.
</p>
<dl class="meta-grid">
  <dt>Java</dt>
  <dd>
    Recording the application or Spring Boot configuration selected in the title bar runs it under JDK Flight Recorder, in the Run console like any other run, and reads
    the recording when the program exits. <strong>Stop and read</strong> takes the recording from the live JVM and then ends the run. <strong>Attach</strong> (in the Profile… dialog) records inside a JVM that is
    already running (listed from <code>jcmd</code>); it needs a JDK with <code>jcmd</code>, and the target must run as you.
  </dd>
  <dt>Node</dt>
  <dd>
    In the Profile… dialog, give a command (<code>node server.js</code>, <code>npm run dev</code>, an Angular or Vite dev server). It runs with <code>NODE_OPTIONS=--cpu-prof</code>, so every Node process it
    starts writes a <code>.cpuprofile</code> <em>when it exits</em> — stop a dev server with its own shutdown (Ctrl+C in the console). Killing it leaves no profile. Frames are reported at
    the file V8 ran; source maps are not applied, so a bundled application shows its bundle.
  </dd>
  <dt>Rust</dt>
  <dd>
    A Cargo run configuration is built first and its binary — not cargo — is what samply records. A command can also be given in the Profile… dialog. Uses <a href="https://github.com/mstange/samply" target="_blank" rel="noreferrer">samply</a>. Bennu picks the first one it finds, in this order: the executable you chose in the panel,
    the copy Bennu downloaded, then <code>samply</code> on PATH. The panel shows which one is in use and its version. The command is wrapped in
    <code>samply record --save-only</code>. Build with debug info (<code>[profile.release] debug = true</code>); without it frames are raw addresses and the recording says so.
  </dd>
  <dt>Getting samply</dt>
  <dd>
    On the Rust tab of the Profile… dialog, <strong>Choose…</strong> picks an executable you already have; Bennu runs <code>samply --version</code> on it and shows the version it found. <strong>Download samply</strong> fetches the latest official release
    (github.com/mstange/samply) for your operating system and CPU, after showing the version, file, size and source. The download is checked against the SHA-256 published with the release; if none can be
    obtained, Bennu says so and installs only if you tick the explicit confirmation. The copy goes into Bennu's data folder and can be updated or removed from the same row. On macOS and Linux unpacking the
    <code>.tar.xz</code> uses the system <code>tar</code> (with xz support); on Windows the <code>.zip</code> is unpacked by Bennu. A path you chose always wins over the downloaded copy, and a chosen path that no longer exists is reported rather than skipped.
  </dd>
  <dt>Import</dt>
  <dd>The upload button reads a <code>.jfr</code>, a <code>.cpuprofile</code> or a samply profile (<code>.json</code> or gzip-compressed <code>.json.gz</code>) made elsewhere. Compressed files are recognised by content, and a profile that
    expands past 512 MB is refused.</dd>
</dl>

<h2>Which JDKs can be recorded</h2>
<p>Bennu checks the project's JDK before recording and, when it cannot, says why and what to do — it never fails silently.</p>
<table>
  <thead><tr><th>JDK</th><th>Recording</th><th>Reading the file</th></tr></thead>
  <tbody>
    <tr><td>12 and newer</td><td>yes</td><td>the JDK's own <code>jfr</code> tool</td></tr>
    <tr><td>11</td><td>yes</td><td><code>jfr</code> of another installed JDK 12+, otherwise a built-in reader run with <code>java</code></td></tr>
    <tr><td>8u262 and newer</td><td>yes</td><td>a built-in reader compiled with <code>javac</code> (a JRE alone cannot)</td></tr>
    <tr><td>8 before 8u262</td><td>no</td><td>Oracle's older builds need a commercial license flag, which Bennu does not enable; update the JDK</td></tr>
    <tr><td>9, 10</td><td>no</td><td>Flight Recorder was open-sourced in 11</td></tr>
    <tr><td>OpenJ9, IBM Semeru</td><td>no</td><td>they have no Flight Recorder; use a HotSpot JDK (Temurin, Corretto, Zulu, Oracle)</td></tr>
    <tr><td>jlink image without <code>jdk.jfr</code></td><td>no</td><td>rebuild it with <code>--add-modules jdk.jfr</code></td></tr>
  </tbody>
</table>

<h2>Reading a recording</h2>
<dl class="meta-grid">
  <dt>Flame graph</dt>
  <dd>
    Width is share of the weight. Click a box to open its source; double-click or <kbd>Enter</kbd> zooms; <kbd>Backspace</kbd> zooms out; arrows move between boxes; the search dims what does not
    match; the thread picker narrows to one thread. Hover a box for its tooltip — it follows the pointer when the graph is scrolled. <strong>What the boxes weigh</strong> is a choice:
    <strong>CPU</strong> (where Java code is running), <strong>Waiting</strong> (time blocked off the CPU — a query waiting for the database is here, and nowhere in the CPU graph) or
    <strong>Allocations</strong> (bytes, by who asked for them). With <strong>Heat</strong> on, a CPU box is coloured by the share of the run it spent itself and everything unremarkable is grey; off, boxes take a colour
    per package. The class filter takes the other frames <em>out of every stack</em>, so with <code>it.acme.*</code> the graph is your code only; the root keeps the whole recording and says how much the filter hides.
  </dd>
  <dt>Hot methods</dt>
  <dd>Self time (in the method itself) and total time (with its callees), the hottest source lines of each, and the method's package. Self time is drawn as a bar and coloured by its level; the bars are scaled to the hottest
    method on screen. <kbd>Enter</kbd> opens the source. A recursive method is counted once per call path.</dd>
  <dt>Memory</dt>
  <dd>
    <strong>From your code</strong> answers which of <em>your</em> methods allocates: JFR reports an allocation at the code that performed it (<code>Arrays.copyOf</code>), so the allocation's whole stack is walked up to the
    nearest frame of the project and the bytes are credited there — <code>Service.load</code> allocated 30 MB, mostly <code>byte[]</code>. A click opens the method. <strong>Allocated types</strong> and
    <strong>Allocation sites</strong> are the raw views, each row with its share of everything allocated; <strong>GC pauses</strong> shows each collector with the share of the run it stopped the program. These are samples, not
    an exact count — a large number of small allocations is under-represented. A recording made before allocation stacks were kept can show types and sites only.
  </dd>
  <dt>Waits</dt>
  <dd>
    <strong>I/O and parks</strong> is time blocked off the CPU: waiting on a socket (the database), a file, a pooled connection. Each row names what was waited on (<code>db.internal:5432</code>), the method that waited longest and the
    total, coloured by its level. <strong>Lock contention</strong> is the monitors threads queued for. JFR reports socket, file and park waits of 10 ms or more (1 ms or more with a shorter sampling interval on JDK 17+) and lock waits over 20 ms.
  </dd>
  <dt>Compare</dt>
  <dd>Against an earlier recording of the same language, by change in each method's share of the run (percentage points), so recordings of different lengths compare fairly.</dd>
</dl>

<h2>Filtering and colours</h2>
<p>
  Every tab starts with a search over its names, a <strong>Packages</strong> field and a <strong>My code</strong> switch. <strong>My code</strong> hides the JDK and the libraries, using the classes the project index knows.
  The field is typed by hand, like a search: <code>it.acme.*, !*.generated.*</code>. A name without a wildcard covers everything inside it (<code>it.acme</code> covers <code>it.acme.web</code> but not <code>it.acmeco</code>); <code>*</code> matches
  any run of characters and <code>?</code> one, so <code>*.service.*</code> is any class in a <code>service</code> package and <code>*Dao</code> any class ending in <code>Dao</code>; a leading <code>!</code> hides instead of keeps. Separate patterns
  with commas or spaces. While you type, the grey text after the caret is the longest package every candidate agrees on — taken from the project's own packages, never guessed — and <kbd>Tab</kbd> (or <kbd>→</kbd>) takes it. The filter applies as you
  type. In the tables the rows are hidden and shares stay relative to the whole recording. The filter is remembered per project; the search is not.
</p>
<p>
  Yellow, orange and red mark readings that deserve a look: a method's self time, a type's share of allocations, a collector's pause share, time blocked on a target, and an increase in <em>Compare</em>.
  A key beside each table says where each colour starts. The sliders button in the panel header edits those values — one scale per kind of reading, since their units differ. They are saved with the project in
  <code>.arbor/bennu/config.toml</code> (<code>[profiler]</code>), together with the class filter, so a team shares one idea of "hot".
</p>
<p>
  <kbd>Ctrl</kbd> + <kbd>F</kbd> inside the panel focuses the search. From the command palette: <strong>Profiler: filter by name</strong>, <strong>Profiler: only this project's classes</strong> (and <strong>show all classes</strong>),
  <strong>Profiler: clear the class filter</strong>, <strong>Profiler: colour thresholds…</strong> and <strong>Profiler: show / hide the numbers in the editor</strong>.
</p>

<h2>Numbers in the editor</h2>
<p>
  After a recording, <strong>every method</strong> carries a mark after its signature: what it earned — <code>38% CPU · 120 ms self · 4.2 MB alloc (byte[]) · 1.2 s waiting on db.internal:5432</code> — or <code>not sampled</code> in grey when the
  recording never caught it. That is not the same as fast: the CPU sampler takes a stack every ~10 ms of the threads that are <em>running</em>, so a method shorter than that, one that did not run in the recording, or one whose whole cost is waiting
  for the database leaves no CPU sample. Its wait shows up as <em>waiting</em> (and in the Waits tab) when it lasted 10 ms or more.
</p>
<p>
  A <strong>line</strong> that stands out — on CPU, on allocation or on waiting — is labelled in place, on the line that causes it (<code>12% CPU · 4.2 MB alloc · 1.2 s wait</code>; allocation and wait include what the line calls), and a gutter bar
  marks the others. Both are coloured on the same yellow / orange / red scale as the tables, by the worst of the three, and grey while nothing stands out; changing the thresholds redraws them. An allocation is credited to the nearest
  method of your project, not to the JDK code that performed it. Hover for the details. Matching is by qualified name from the current text of the file, so the marks follow the code while you edit; if the file changed after the recording they
  are drawn dimmed and labelled stale. The eye button in the panel header, <strong>Profiler: show / hide the numbers in the editor</strong> in the palette, or <em>Settings › Editor › Profiler hints</em> turns them on and off.
</p>

<h2>Sampling interval</h2>
<p>
  The <strong>Profile…</strong> dialog sets the CPU sampler's interval for the next Java recording: 10 ms (the default), 5, 2 or 1 ms. A shorter interval catches shorter methods and also reports shorter waits on sockets and files, at the cost of
  more overhead. It needs JDK 17 or newer; on an older one the recording keeps 10 ms. A launched JVM also records stacks up to 256 frames deep (JFR's default is 64, which a Spring request outgrows, leaving the allocation or the wait with no project
  frame to be credited to); attaching to a running JVM cannot change that depth.
</p>

<Callout variant="info">
  Samples are statistical: a method that ran for a few milliseconds may not appear, and a short recording is noisy. Record a representative scenario for at least a few seconds.
</Callout>

<h2>For Imber</h2>
<p>Imber, the assistant, can read the newest recording (hottest methods, allocation sites, lock waits, GC) to propose a change. It only reads; it never starts a recording. See the page <strong>Imber: Bevy games and the profiler</strong>.</p>

<h2>What is not claimed</h2>
<ul>
  <li>Bennu downloads samply on request, but does not install Node or a JDK for you.</li>
  <li>Native frames inside the JVM (garbage collector threads, JIT) are not sampled by Flight Recorder's Java sampler.</li>
  <li>Source maps (Node) and inlined frames (Rust) are shown as the runtime reports them.</li>
</ul>
