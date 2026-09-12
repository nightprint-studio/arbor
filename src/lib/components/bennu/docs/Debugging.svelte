<script lang="ts">
  /**
   * Debugging: starting a session and setting breakpoints, several sessions at once, Rust through a debug adapter and its
   * formatters, where a breakpoint can go and what its dot says, the list, muting, and conditions with pass counts.
   */
  import Callout from '$lib/components/shared/ui/Callout.svelte';
</script>

<span class="eyebrow">Build, run &amp; test</span>
<h1>Debugging</h1>

<p class="doc-lead">
  Stop the program where you want it and read what it is actually holding — Java over JDWP, Rust through a debug adapter, with the same panel and the same gestures for both.
</p>

<h2>Starting</h2>
<table>
  <thead><tr><th>Gesture</th><th>Does</th></tr></thead>
  <tbody>
    <tr><td><kbd>Shift</kbd> + <kbd>F9</kbd>, or 🐞 beside ▶</td><td>Launches the active configuration with a debugger attached</td></tr>
    <tr><td>A click in the left margin, outside the line numbers</td><td>Sets or clears a breakpoint</td></tr>
    <tr><td><kbd>Ctrl</kbd> + <kbd>F8</kbd></td><td>Sets or clears one on the caret's line</td></tr>
    <tr><td><kbd>Ctrl</kbd> + <kbd>Shift</kbd> + <kbd>F8</kbd></td><td>The breakpoint list</td></tr>
  </tbody>
</table>
<p>
  Breakpoints belong to the <em>project</em>, not to a session: they are kept in <code>.arbor/bennu/config.toml</code> beside the run configurations, still there tomorrow, and a
  launch installs whatever is set. What happens once the program stops — frames, values, watches — is <strong>Frames, values &amp; watches</strong>.
</p>

<h3>More than one at once</h3>
<p>
  Several programs can be under the debugger together, each in its own console tab. A session and its tab are one thing, so the panel shows the one whose tab is in front, and moving
  along the strip moves the frames, variables and watches with it. What the VM made of each breakpoint is per session — two VMs on one project can disagree, one having loaded the class.
</p>
<Callout variant="tip" title="A stop pulls you to it">
  When a breakpoint fires in a program you were not reading, its tab comes forward with the window — having to hunt for which of three consoles it happened in would be the one case where
  the debugger makes you work for it.
</Callout>

<h2>Rust, and the debug adapter</h2>
<p>
  A Cargo configuration debugs too, in the same panel with the same keys. Underneath it differs: a JVM is debugged by <em>launching it differently</em> — an agent argument, and the VM
  connects back — while a native binary's debugger must be what <strong>starts</strong> the process. So a Rust debug launch:
</p>
<ol class="step-list">
  <li>Builds the target, its errors going to the console under the same tab — a failing build reads as one.</li>
  <li>Reads the executable's real path from cargo's own JSON. <code>target/debug/«name»</code> is wrong on any project configuring anything — a named profile, a <code>[[bin]]</code>
    named apart from its package, a <code>target-dir</code>, a cross target — and a test binary's name carries an unpredictable hash.</li>
  <li>Hands that path to a <strong>debug adapter</strong>.</li>
</ol>
<table>
  <thead><tr><th>Adapter, in order of preference</th><th>Why there</th></tr></thead>
  <tbody>
    <tr><td><strong>CodeLLDB</strong></td><td>It has <strong>Rust data formatters</strong>: a <code>Vec&lt;T&gt;</code> shows its elements, an <code>Option</code> <code>Some(3)</code>, a <code>String</code> its text. It ships as a VS Code extension, so the extension directory is searched as well as <code>PATH</code>.</td></tr>
    <tr><td><strong>lldb-dap</strong></td><td>LLVM's own, on most Macs without installing anything — Xcode's toolchain included, which no windowed app's <code>PATH</code> has. Its Rust formatters come from the toolchain (below).</td></tr>
    <tr><td><strong>GDB</strong></td><td>In DAP mode, GDB 14 or newer — the fallback on a Linux machine with no LLVM.</td></tr>
  </tbody>
</table>
<Callout variant="info" title="Pinning one">
  <strong>Settings → Java → Debugger</strong> can pin an adapter. A pinned one that is missing is reported, not quietly replaced: the three render values differently, and one you did not choose would
  make the variables panel disagree with itself between machines.
</Callout>

<h3>Why a <code>Vec</code> shows its elements</h3>
<p>
  Neither LLDB nor GDB knows Rust's types. Stopped on a <code>Vec&lt;Order&gt;</code>, an unconfigured LLDB shows <code>buf</code> → <code>inner</code> → <code>ptr</code> →
  <code>pointer</code> → an address — five clicks down to nothing. <code>String</code> is a byte buffer, <code>HashMap</code> a table's internals, <code>Option</code> a discriminant
  and a union, <code>Rc</code> a control block. One fix per adapter, applied for you:
</p>
<dl class="meta-grid">
  <dt>CodeLLDB</dt>
  <dd>Ships Rust formatters, and is told the source language so it uses them.</dd>
  <dt>lldb-dap</dt>
  <dd>Ships none — but the <strong>Rust toolchain does</strong>. <code>rust-lldb</code> is, entirely, a script loading two files from <code>lib/rustlib/etc</code> into a plain LLDB; Bennu loads the same two at launch. Without a toolchain, the variables tree says at the top what to install.</dd>
  <dt>GDB</dt>
  <dd>Reads the printers the binary names and understands Rust as a language, so it is left to its own auto-loading.</dd>
</dl>
<p>
  Formatters cover the standard library. A <code>struct</code> of your own has none anywhere, and LLDB's default prints <em>nothing</em> on its row. On <code>lldb-dap</code> Bennu turns
  on synthesised summaries — <code>{'{'}id:7, total:19.9{'}'}</code> — and where even that is unavailable the row says how many fields it holds.
</p>
<Callout variant="info" title="Two differences on a native session">
  A frame carries <strong>one name</strong> — <code>geode::mine::dig</code> — not a class and a method, since splitting it would invent nonsense on synthetic frames. And the session follows
  the <strong>thread that stopped</strong>; choosing among threads is not offered yet.
</Callout>

<h2>Breakpoints</h2>
<h3>Where one can go</h3>
<p>
  A Java breakpoint is a position in <em>bytecode</em>, so only a line compiling to some takes one. A package statement, an annotation, a class or method signature, a field with nothing to
  run, a comment or a lone brace compile to nothing, and the margin does not offer a breakpoint there — the faint dot under the pointer appears only on lines that can take one.
</p>
<ul>
  <li>A field <em>with</em> an initializer is offered, since its initializer runs; so is any statement.</li>
  <li>A breakpoint already set stays clickable when an edit makes its line unqualified — otherwise you could not remove it.</li>
  <li>Breakpoints follow their lines as you edit above them. One landing on a line with no code binds to the statement below, and the tooltip names the line it really stops on.</li>
</ul>

<h3>What the dot says</h3>
<table>
  <thead><tr><th>Dot</th><th>Means</th></tr></thead>
  <tbody>
    <tr><td><strong>Solid</strong></td><td>The VM accepted it; the program will stop there.</td></tr>
    <tr><td><strong>Hollow</strong></td><td>The class is not loaded yet — resolves itself the moment the program touches it.</td></tr>
    <tr><td><strong>Grey outline</strong></td><td>Disabled. Right-click a breakpoint to disable one you will want back in ten minutes.</td></tr>
    <tr><td><strong>A ring</strong></td><td>It carries a condition or a pass count — below.</td></tr>
  </tbody>
</table>

<h3>The list, exceptions and muting</h3>
<p>
  <kbd>Ctrl</kbd> + <kbd>Shift</kbd> + <kbd>F8</kbd> lists every breakpoint in the project, grouped by file, with a switch to disable and a bin to remove. It is also where to add an
  <strong>exception breakpoint</strong>, which stops where a throwable is <em>thrown</em> rather than caught — the only way to see the state that produced it, with no line to click.
</p>
<p>
  The ⊘ in the debugger's controls <strong>mutes</strong> every breakpoint: they stay set and listed, and the program runs to its end at full speed. Muting lasts for the session — a
  debugger silently ignoring your breakpoints tomorrow would be a trap. It works on both kinds of session: a JVM's breakpoints are uninstalled from the VM, a native session's removed from
  the adapter and put back on unmute. Either way the list is untouched.
</p>

<h2>Conditions, and stopping every Nth time</h2>
<p>
  Right-click a breakpoint → <em>Add condition…</em>, or use the breakpoint list, where each has a condition field and a pass count. A breakpoint with either is drawn ringed — a
  breakpoint that does not stop is the most expensive thing to misread in a debugger, and "it has a condition" is usually the answer.
</p>
<p>
  <strong>Stop on every Nth hit</strong> counts <em>after</em> the condition — "the third time <code>i&nbsp;&gt;&nbsp;5</code>" — the only reading that composes. The count restarts each
  launch, and the list shows how many times each breakpoint has stopped the program: also the quickest answer to "is this line even running".
</p>
<Callout variant="warning" title="A condition costs what it costs">
  The VM stops every time the line is reached, the condition is evaluated in that frame, and the program is let go when it does not hold — as in every debugger. On a line run a million
  times, it is slow.
</Callout>

<h3>What a Java condition may say</h3>
<p>A <strong>path compared with a literal</strong> — the paths a watch takes — joined with <code>&amp;&amp;</code>, <code>||</code>, <code>!</code> and parentheses:</p>
<table>
  <thead><tr><th>Condition</th><th>Note</th></tr></thead>
  <tbody>
    <tr><td><code>i &gt; 5</code>, <code>count == 0</code>, <code>ratio &lt; 0.5</code></td><td>—</td></tr>
    <tr><td><code>order.customer.name == "acme"</code>, <code>items[2].price &gt; 0</code></td><td>Fields and subscripts</td></tr>
    <tr><td><code>order != null &amp;&amp; order.total &gt; 100</code></td><td><code>&amp;&amp;</code> short-circuits, so the left guards the right</td></tr>
    <tr><td><code>done</code>, <code>!order.paid</code></td><td>A path alone must be a boolean</td></tr>
    <tr><td><code>status.name == "ACTIVE"</code></td><td>An enum: every constant carries its <code>name</code></td></tr>
  </tbody>
</table>
<Callout variant="info" title="Deliberately not Java">
  A condition quietly answering something adjacent to what you typed swallows the stop, with nothing on screen. So <strong>method calls</strong> — <code>list.size() &gt; 3</code> runs
  application code inside a paused program — and <strong>arithmetic</strong> — <code>i + 1 == n</code> — are refused by name as you type, not approximated.
</Callout>
<p>
  When a condition cannot be answered at a hit — a null halfway down the path, a field missing on this subclass — the program <strong>stops anyway</strong> and the breakpoint says why. A bug in
  a condition is only visible from where it happened; running on would turn a typo into a breakpoint that never fires and never explains itself.
</p>

<h3>On a Rust session</h3>
<p>
  The condition is the <strong>debug adapter's</strong> own expression language, sent untouched — it has a real evaluator and documentation, and a subset reimplemented here would be worse and
  disagree with all you have read about LLDB or GDB. What it may say is CodeLLDB's, lldb-dap's or GDB's documentation; the status bar names which is driving.
</p>
<ul>
  <li>Bennu <strong>cannot check it as you type</strong> — it has no parser for a language it does not own — so a mistake shows after launch, as an unverified breakpoint with the adapter's
    complaint in its tooltip.</li>
  <li>The <strong>pass count</strong> goes as that adapter's hit-condition expression, <code>%3</code> for every third, which not all read alike: a breakpoint stopping every time is the adapter
    not doing it. An adapter declaring it supports neither has the field dropped instead, because a request setting a file's breakpoints sets <em>all</em> of them.</li>
</ul>
