<!-- Bennu docs — the debugger: breakpoints, frames, values and watches. -->
<h1>Debugging</h1>
<p class="doc-lead">
  Stop the program where you want it and read what it is actually holding — for Java over JDWP,
  for Rust through a debug adapter, with the same panel and the same gestures for both.
</p>

<h2>Debugging</h2>
<p>
  <kbd>Shift</kbd> + <kbd>F9</kbd>, or the 🐞 beside ▶, launches the same configuration with a
  debugger attached. Click in the left margin — outside the line numbers — to set a breakpoint;
  <kbd>Ctrl</kbd> + <kbd>F8</kbd> sets one on the caret's line. Breakpoints belong to the
  <em>project</em>, not to a session: they are kept in <code>.arbor/bennu/config.toml</code> beside the
  run configurations, so they are still there tomorrow, and a launch installs whatever is set.
</p>
<h3>More than one at once</h3>
<p>
  Several programs can be under the debugger at the same time, each in its own console tab. A
  session and its tab are the same thing, so the panel shows the one whose tab is in front, and
  moving along the strip moves the frames, the variables and the watches with it. What the running
  VM made of each breakpoint is per session too — two VMs on the same project can disagree, because
  one has loaded the class and the other has not yet.
</p>
<p>
  <strong>A stop pulls you to it.</strong> When a breakpoint fires in a program you were not
  reading, its tab comes forward with the window — a breakpoint is the answer to something that
  just happened, and having to go looking for which of three consoles it happened in would be the
  one case where the debugger makes you work for it.
</p>

<h3>Rust, and the debug adapter</h3>
<p>
  A Cargo configuration debugs too, and the panel is the same one: the same frames, the same
  variables tree, the same watches, the same keys. What differs is underneath.
</p>
<p>
  A JVM is debugged by <em>launching it differently</em> — an agent argument, and the VM connects back
  to Bennu. A native binary cannot be: the debugger has to be the thing that <strong>starts</strong>
  the process, so the target must be built first and its path known. So a Rust debug launch builds,
  reads the executable's real path out of cargo's own JSON, and hands that to a
  <strong>debug adapter</strong>. The build's errors go to the console under the same tab, so a
  failing build reads as a failing build rather than as a debugger that would not start.
</p>
<p>
  The path is read from cargo rather than composed, because <code>target/debug/&lt;name&gt;</code> is
  wrong on any project that configures anything — a named profile, a <code>[[bin]]</code> whose name
  differs from the package's, a <code>target-dir</code>, a cross-compilation target — and a test
  binary's name has a hash in it that is not predictable at all.
</p>
<p>Bennu drives whichever adapter is installed, preferring them in this order:</p>
<table>
  <thead><tr><th>Adapter</th><th>Why that order</th></tr></thead>
  <tbody>
    <tr>
      <td><strong>CodeLLDB</strong></td>
      <td>The one with <strong>Rust data formatters</strong>: a <code>Vec&lt;T&gt;</code> shows its
        elements, an <code>Option</code> shows <code>Some(3)</code>, a <code>String</code> shows its
        text. Without those a debugger technically works and practically does not, which is why it is
        first. It ships as a VS Code extension, so it is looked for in the extension directory as well
        as on <code>PATH</code>.</td>
    </tr>
    <tr>
      <td><strong>lldb-dap</strong></td>
      <td>LLVM's own, and present on most macOS machines without installing anything — including
        inside Xcode's toolchain, which is on no windowed app's <code>PATH</code>. It ships no Rust
        formatters of its own, so Bennu loads the <em>toolchain's</em> into it at launch — see below.</td>
    </tr>
    <tr><td><strong>GDB</strong></td><td>In DAP mode, which needs GDB 14 or newer. The fallback on a
      Linux machine with no LLVM.</td></tr>
  </tbody>
</table>
<p>
  <strong>Settings → Debugger</strong> can pin one. A pinned adapter that is missing is reported rather
  than quietly replaced: the three render values differently, and debugging with one you did not choose
  would make the variables panel disagree with itself between machines.
</p>
<h3>Why a <code>Vec</code> shows its elements</h3>
<p>
  Neither LLDB nor GDB knows anything about Rust's types by itself. Stopped on a
  <code>Vec&lt;Order&gt;</code>, an unconfigured LLDB shows <code>buf</code> → <code>inner</code> →
  <code>ptr</code> → <code>pointer</code> → an address, and the elements are nowhere: five clicks down
  a chain of implementation details to reach nothing. The same goes for <code>String</code> (a byte
  buffer), <code>HashMap</code> (a hash table's internals), <code>Option</code> (a discriminant and a
  union), <code>Rc</code> (a control block).
</p>
<p>
  Three different things fix that, one per adapter, and Bennu does whichever applies:
</p>
<ul>
  <li><strong>CodeLLDB</strong> ships Rust formatters itself, and is told the source language so it
    uses them.</li>
  <li><strong>lldb-dap</strong> ships none — but the <strong>Rust toolchain does</strong>. What
    <code>rust-lldb</code> is, in its entirety, is a script that loads two files out of
    <code>lib/rustlib/etc</code> into a plain LLDB; Bennu loads the same two at launch. So a machine
    with a Rust toolchain gets Rust-shaped values out of LLVM's own adapter, and one without gets a
    warning at the top of the variables tree saying what to install.</li>
  <li><strong>GDB</strong> reads the printers the binary itself names and understands Rust as a
    language, so it is left to its own auto-loading.</li>
</ul>
<p>
  Formatters cover the standard library. A <code>struct</code> of your own has no formatter anywhere,
  and LLDB's default there is to print <em>nothing</em> on the row that names the variable — the
  fields are underneath, but the row reads as empty. On <code>lldb-dap</code> Bennu turns on
  synthesised summaries so it reads as <code>{'{'}id:7, total:19.9{'}'}</code>; where even that is
  unavailable the row says how many fields are inside rather than showing a blank cell.
</p>

<p>
  Two things differ in the panel on a native session, and both are honest about it. A frame carries
  <strong>one name</strong> rather than a class and a method — <code>geode::mine::dig</code> — because
  that is what the debugger reports and splitting it to invent a class produces nonsense on the
  synthetic frames. And the session follows the <strong>thread that stopped</strong>; picking among
  several threads is not offered yet.
</p>
<p>
  Muting works on both, by different means: a JVM's breakpoints are uninstalled from the VM, a native
  session's are removed from the adapter and put back on unmute. Either way the <em>list</em> is
  untouched, which is the point — the twelve breakpoints you will want back in a minute are still
  there.
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

<h3>Conditions, and stopping every Nth time</h3>
<p>
  Right-click a breakpoint and choose <em>Add condition…</em>, or open the Breakpoints window
  (<kbd>Ctrl</kbd> + <kbd>Shift</kbd> + <kbd>F8</kbd>) where every breakpoint has a condition field
  and a pass count. A breakpoint that carries either is drawn with a ring around its dot, because a
  breakpoint that does not stop is the most expensive thing to misread in a debugger and "it has a
  condition on it" is the answer most of the time.
</p>
<p>
  <strong>Stop on every Nth hit</strong> is the field beside it. It counts <em>after</em> the
  condition — “the third time <code>i&nbsp;&gt;&nbsp;5</code>” — which is the only reading that
  composes. The count starts again on each launch, and the Breakpoints window shows how many times
  each one has actually stopped the program, which is also the quickest answer to “is this line even
  running”.
</p>
<p>
  A condition costs what it costs: the VM stops every time the line is reached, the condition is
  evaluated in that frame, and the program is let go again when it does not hold. That is how every
  debugger does it. On a line that runs a million times, it will be slow.
</p>
<h4>What a Java condition may say</h4>
<p>
  A <strong>path compared with a literal</strong> — the same paths a watch takes, joined with
  <code>&amp;&amp;</code>, <code>||</code>, <code>!</code> and parentheses:
</p>
<ul>
  <li><code>i &gt; 5</code>, <code>count == 0</code>, <code>ratio &lt; 0.5</code></li>
  <li><code>order.customer.name == "acme"</code>, <code>items[2].price &gt; 0</code></li>
  <li><code>order != null &amp;&amp; order.total &gt; 100</code> — <code>&amp;&amp;</code>
    short-circuits, so the left side guards the right</li>
  <li><code>done</code>, <code>!order.paid</code> — a path on its own has to be a boolean</li>
  <li><code>status.name == "ACTIVE"</code> for an enum: every constant carries its own
    <code>name</code>, and reading it is an ordinary field walk</li>
</ul>
<p>
  It is deliberately not Java. A watch is a path for the reasons that page gives, and the argument
  is stronger for a condition: a watch that quietly answers something adjacent to what you typed
  shows you a wrong number, which you might notice — a condition that does it swallows the stop, and
  there is nothing on screen at all. So <strong>method calls</strong> (<code>list.size() &gt; 3</code>
  — calling into a paused program runs application code inside it) and <strong>arithmetic</strong>
  (<code>i + 1 == n</code>) are refused by name, as you type, rather than approximated.
</p>
<p>
  When a condition cannot be answered at a hit — a null halfway down the path, a field that is not
  there on this subclass — the program <strong>stops anyway</strong> and the breakpoint says why. A
  bug in a condition is only visible from where it happened, and silently running on would turn a
  typo into a breakpoint that never fires and never explains itself.
</p>
<h4>On a Rust session</h4>
<p>
  The condition is the <strong>debug adapter's</strong> own expression language, sent to it
  untouched — it already has a real evaluator, its documentation describes it, and a reimplemented
  subset here would be a worse language that also disagreed with everything you have read about
  LLDB or GDB. So the answer to “what may it say” is CodeLLDB's, lldb-dap's or GDB's documentation,
  depending on which one is driving; the status bar names it.
</p>
<p>
  Two consequences worth knowing. Bennu <strong>cannot check it as you type</strong> — it has no
  parser for a language it does not own — so a mistake shows up after the launch, as a breakpoint
  that did not verify with the adapter's complaint in its tooltip. And the <strong>pass count</strong>
  is sent as that adapter's own hit-condition expression (<code>%3</code> for every third), which not
  all of them read the same way; a breakpoint that stops every time when you asked for every third is
  the adapter saying it does not do this rather than the setting being lost. An adapter that says
  outright it supports neither has the field dropped instead: the breakpoint itself must survive,
  because a request that sets a file's breakpoints sets <em>all</em> of them.
</p>
