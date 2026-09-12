<script lang="ts">
  /**
   * Frames, values and watches: where the debugger puts what it found in the Run console, stepping and step filters, reading
   * a value whole, watches and evaluators, and what is worth knowing.
   */
  import Callout from '$lib/components/shared/ui/Callout.svelte';
</script>

<span class="eyebrow">Build, run &amp; test</span>
<h1>Frames, values &amp; watches</h1>

<p class="doc-lead">
  Where the debugger puts what it found, and how to read a value that is bigger than the row it sits in.
</p>

<h2>It all happens in the Run console</h2>
<p>
  There is no separate Debug window: debugging is the same launch with more to look at, so <strong>Run</strong> (<kbd>Alt</kbd> + <kbd>R</kbd>) grows what the moment needs. While a
  session is attached, the transport controls sit on the left of the status row. While the program is <em>stopped</em>, two columns open left of the transcript:
</p>
<div class="feature-grid two-col">
  <div class="feature-card">
    <div class="fc-eyebrow">What got here</div>
    <div class="fc-title">Frames</div>
    <div class="fc-desc">Library and JDK frames are muted and open their source or a decompiled stub. A run of consecutive library frames <strong>folds into one row</strong> — a stop inside a framework is forty frames around your three; ⊟ turns folding off.</div>
  </div>
  <div class="feature-card">
    <div class="fc-eyebrow">In the selected frame</div>
    <div class="fc-title">Variables and watches</div>
    <div class="fc-desc">The frame's variables, with your watches underneath — both read again when you pick another frame.</div>
  </div>
</div>
<ul>
  <li><strong>The editor follows the debugger.</strong> Every stop and step opens the current frame's file at its line — stepping into a method changes the tab for you — and the stopped line
    is banded full width with a bar down its left edge.</li>
  <li>Both columns are <strong>resizable</strong>, and each collapses to a labelled strip from the ⇤ in its header, giving the transcript the rest. Where you left them is remembered.</li>
  <li>When the program stops, <strong>Bennu comes to the front</strong> and opens the console: a breakpoint fires because of something in another window, and the editor is where the answer
    is. A step does not raise it — you did that here.</li>
</ul>

<h2>Stepping</h2>
<table>
  <thead><tr><th>Key</th><th>Does</th></tr></thead>
  <tbody>
    <tr><td><kbd>F8</kbd></td><td>Step over</td></tr>
    <tr><td><kbd>F7</kbd></td><td>Step into</td></tr>
    <tr><td><kbd>Shift</kbd> + <kbd>F8</kbd></td><td>Step out</td></tr>
    <tr><td><kbd>F9</kbd></td><td>Resume</td></tr>
  </tbody>
</table>
<p>
  A step <strong>passes straight through</strong> the JDK, Spring's AOP machinery, Reactor and the logging façades, rather than stopping in <code>ArrayList.add</code> or walking
  <code>ReflectiveMethodInvocation.proceed</code> a dozen times. Landing in a <strong>proxy</strong> — a Spring CGLIB or Hibernate stand-in, methods existing only at runtime — it keeps going,
  so stepping into <code>service.place(order)</code> arrives at <code>place</code>.
</p>
<Callout variant="tip" title="Stepping into Spring itself">
  Which packages are passed through is a judgement about whose code you are debugging: <strong>Settings → Java → Debugger</strong> lists them, and removing <code>org.springframework.*</code> steps
  into Spring. A pattern may carry <code>*</code> at one end only — the VM refuses anything else, and one bad entry would stop stepping altogether — so it is checked as you type. Changes apply
  to the next launch.
</Callout>

<h2>Reading a value whole</h2>
<p>
  The variables tree fetches a row's contents only when opened — a stopped program is a graph, and walking it eagerly would be a round trip per node nobody looked at. That is wrong for one job:
  a struct of fifteen fields, four of them structs, is <strong>nineteen disclosure triangles</strong> before you can read it, and then no longer fits on screen.
</p>
<p>
  So every row with something inside carries a <strong>{'{}'}</strong> button — on hover and on keyboard focus — reading the value and everything under it at once, as text you can scroll, search
  with the editor's find, and copy. The tree is for looking <em>around</em>; this is for looking <em>at</em>. It works on a watch row too.
</p>
<dl class="meta-grid">
  <dt>RON</dt>
  <dd>The value is a Rust value, and RON keeps the three distinctions JSON throws away: a struct is not a map, a tuple <code>(1, "x")</code> is not a list, a variant is a name. It is RON-<em>shaped</em>, as the footer says — a debugger reports names, rendered values and children, not a type system — for reading, not parsing.</dd>
  <dt>Literals stay literals</dt>
  <dd>A value the debugger already rendered as a literal is printed, not opened. Rust's formatters render a <code>String</code>, <code>PathBuf</code> or <code>OsString</code> as text <em>and</em> offer children — a byte buffer underneath — and following those turns a path into four levels of internals and a row per character. A <code>size=3</code> describes a container, so it still opens.</dd>
  <dt>Bounded, and saying so</dt>
  <dd>Each value is a round trip against a suspended program, so the walk stops at a depth, a node count, a container width or a time budget — and <strong>says</strong> so, at the top and where it stopped. A value containing itself is named as a cycle. The same modal works on a Java object graph.</dd>
</dl>

<h2>Watches</h2>
<p>
  A watch is a <strong>path</strong>: <code>order</code>, <code>order.customer.name</code>, <code>items[2]</code>. On a native session it takes a leading <code>*</code> to follow a reference —
  <code>*head</code> — and <code>*self.next</code> means what the same line means in the source, the star binding looser than the dots.
</p>
<p>
  A path is <strong>read out of the variables tree</strong>, not handed to an evaluator. So <code>v[0]</code> in the watch box and <code>[0]</code> under <code>v</code> are the same row, mean the
  same whichever adapter resolved, and know which variant an enum actually holds — which no static type can. A subscript is fetched alone, so <code>v[400000]</code> on a million-element
  <code>Vec</code> is a fair question with an exact answer. Watches are saved with the breakpoints.
</p>
<p>Anything not a path goes to the debugger's own evaluator, and Bennu says which and what it can do:</p>
<table>
  <thead><tr><th>CodeLLDB prefix</th><th>Evaluator</th></tr></thead>
  <tbody>
    <tr><td><code>/se</code> — the default</td><td>Its own reader: follows the formatters, runs nothing in the program</td></tr>
    <tr><td><code>/nat</code></td><td>LLDB's own parser</td></tr>
    <tr><td><code>/py</code></td><td>Python, with <code>$name</code> from the frame</td></tr>
  </tbody>
</table>
<p>
  The other two adapters have one evaluator, so a prefix they lack is refused by name rather than sent back as a syntax error about a slash.
</p>
<Callout variant="warning" title="Rust method calls cannot be evaluated">
  Not a debugger limitation: a debugger can only call a function that is <em>in the binary</em>, and a generic function nobody called was never compiled. So <code>v.len()</code>, a macro, a
  turbofish, <code>?</code>, <code>.await</code> and a closure each come back with one line saying why, and what to read instead.
</Callout>

<h2>Things worth knowing</h2>
<dl class="meta-grid">
  <dt>Objects show fields, not <code>toString()</code></dt>
  <dd>An object reads <code>Order@1f3c</code> and expands to its real fields, superclasses included. <code>toString()</code> would read better and would run application code in a paused program — blocking on a lock the stopped thread holds, changing state, or throwing.</dd>
  <dt>Variable names need <code>-g</code></dt>
  <dd>Maven compiles with full debug information by default. A class compiled without it still holds breakpoints — the line table is separate — but the panel shows only <code>this</code>.</dd>
  <dt>The launch is not frozen</dt>
  <dd>The program starts and runs, so a start-up breakpoint may pass before the debugger listens. Tick <em>Suspend the VM until the debugger has attached</em> in the configuration, and every debug launch begins stopped, waiting for Resume.</dd>
  <dt>Detach is not Stop</dt>
  <dd>Detaching leaves the program running without a debugger; the console's ■ ends it. A suspended VM holds its locks and port, so a session left paused looks exactly like a hang — the rail's Debug dot turns amber to say so.</dd>
</dl>
