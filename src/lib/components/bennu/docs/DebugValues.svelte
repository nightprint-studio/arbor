<!-- Bennu docs — the debugger's panel: frames, variables, values and watches. -->
<h1>Frames, values &amp; watches</h1>
<p class="doc-lead">
  Where the debugger puts what it found, and how to read a value that is bigger than the row it
  is sitting in.
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

<h3>Reading a value whole</h3>
<p>
  The variables tree fetches a row's contents only when the row is opened, which is the right default —
  a stopped program has an object graph, and walking it eagerly would be a round trip per node for rows
  nobody looked at. It is the wrong shape for one job: a struct with fifteen fields, four of which are
  structs, is <strong>nineteen disclosure triangles</strong> before you can read it, and by the time it
  is open it no longer fits on screen.
</p>
<p>
  So every row with something inside it carries a <strong>{'{}'}</strong> button — on hover, and on
  keyboard focus — that reads the value and everything under it in one go and shows it as text you can
  scroll, search with the editor's own find, and copy. The tree is for looking <em>around</em>; this is
  for looking <em>at</em>. It works on a watch row too, which is where it earns the most.
</p>
<p>
  The text is <strong>RON</strong> — because the value is a Rust value, and RON keeps the three
  distinctions JSON throws away that are exactly the ones a debugger is opened for: a named struct is
  not a map, a tuple <code>(1, "x")</code> is not a list, and an enum variant is a name rather than a
  tag field somebody invented. It is RON-<em>shaped</em> rather than RON-exact, and the footer says so:
  what a debugger reports is a name, a rendered value and a list of children, not a type system, so the
  shape is read off the children. It is for reading, not for feeding to a parser.
</p>
<p>
  A value the debugger already rendered <strong>as a literal</strong> is printed and not opened, even
  when it has something inside it — which is not an optimisation but the difference between reading a
  path and reading its characters. Rust's formatters render a <code>String</code>, a <code>PathBuf</code>
  and an <code>OsString</code> as their text <em>and</em> still offer children, because underneath each
  is a byte buffer; following those turns one path into four levels of internals and then one row per
  character. A <code>size=3</code> is not a literal — it describes a container without being it — so
  those still open.
</p>
<p>
  Every value is a round trip against a suspended program, so the walk stops on whichever of a depth, a
  node count, a container's width or a time budget comes first — and <strong>says</strong> it did, both
  at the top and at the row where it stopped. A dump silently cut reads as a complete answer and would
  be quoted as one. A value that contains itself is named as a cycle rather than followed, and the same
  modal works on a Java object graph, since it walks what both debuggers have in common.
</p>

<h3>Watches</h3>
<p>
  A watch is a <strong>path</strong>: <code>order</code>, <code>order.customer.name</code>,
  <code>items[2]</code> — a variable, then fields and subscripts. On a native session it also takes a
  leading <code>*</code> to follow a reference: <code>*head</code>, and <code>*self.next</code> means
  what the same line means in the source file, since the star binds looser than the dots.
</p>
<p>
  A path is <strong>read out of the variables tree</strong>, not handed to an expression evaluator. So
  <code>v[0]</code> in the watch box and <code>[0]</code> under <code>v</code> in the tree are the
  same row, it means the same thing whichever adapter resolved, and it knows which variant an enum is
  actually holding — which no static type can tell you. A subscript is fetched on its own, so
  <code>v[400000]</code> on a million-element <code>Vec</code> is a fair question with an exact
  answer.
</p>
<p>
  Anything that is <em>not</em> a path goes to the debugger's own evaluator, and Bennu says which one
  and what it can do. CodeLLDB has three, chosen by a prefix: <code>/se</code> (its own reader —
  follows the formatters, runs nothing in the program, and the default), <code>/nat</code> (LLDB's own
  parser) and <code>/py</code> (Python, with <code>$name</code> from the frame). The other two
  adapters have one, so a prefix they do not have is refused by name instead of being sent and coming
  back as a syntax error about a slash.
</p>
<p>
  <strong>Rust method calls cannot be evaluated at all</strong>, and Bennu says so rather than
  forwarding LLDB's complaint about it. The reason is not a limitation of the debugger: a debugger can
  only call a function that is <em>in the binary</em>, and a generic function nobody called was never
  compiled. So <code>v.len()</code>, a macro, a turbofish, <code>?</code>, <code>.await</code> and a
  closure each come back with one line saying why, and what to read instead. Watches are persisted
  with the breakpoints.
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
