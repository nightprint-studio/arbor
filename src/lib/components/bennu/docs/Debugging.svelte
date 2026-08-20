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
