<span class="eyebrow">Imber</span>
<h1>Imber: Debugging with Imber</h1>

<p class="doc-lead">
  With the Debugger family on, Imber can debug a Java program instead of guessing: it sets breakpoints and logpoints, runs the program under the debugger or attaches to one, waits for a stop, reads the stack and values and steps, on the very session the Debug panel shows.
</p>

<p>The debugger itself is described on the <strong>Debugging</strong> and <strong>Frames, values and watches</strong> pages; this page covers what is specific to Imber.</p>

<h2>What it can do</h2>
<table>
  <thead><tr><th>Step</th><th>Tools</th><th>Asks?</th></tr></thead>
  <tbody>
    <tr><td>Set, list, remove breakpoints and logpoints</td><td><code>debug_set_breakpoint</code>, <code>debug_list_breakpoints</code>, <code>debug_remove_breakpoint</code></td><td>No</td></tr>
    <tr><td>Start a session</td><td><code>debug_launch</code> (builds and runs a main class), <code>debug_attach</code> (to a JVM on 127.0.0.1)</td><td>Every time</td></tr>
    <tr><td>Wait and look</td><td><code>debug_wait_stop</code>, <code>debug_stack</code>, <code>debug_variables</code>, <code>debug_evaluate</code>, <code>debug_output</code></td><td>No</td></tr>
    <tr><td>Move</td><td><code>debug_step</code> (several steps at once), <code>debug_resume</code>, <code>debug_run_to</code> (like Run to Cursor), <code>debug_break_on_exception</code></td><td>No</td></tr>
    <tr><td>Let go</td><td><code>debug_detach</code></td><td>No</td></tr>
  </tbody>
</table>

<h2>Its breakpoints are visible and yours</h2>
<p>
  Breakpoints Imber sets are saved in the project's breakpoints, drawn with a sparkle in the gutter and badged <em>Imber</em> in the Breakpoints list (<kbd>Ctrl</kbd> + <kbd>Shift</kbd> + <kbd>F8</kbd>). You can disable or delete them like any other. It never changes or removes one you set, and holds at most 20. A logpoint can say "only when <code>total &gt; 100</code>" through a condition and a pass count, and can ask for values to be captured at the stop.
</p>

<h2>Reading values cannot change anything</h2>
<p>
  Values are read as <em>paths</em> (<code>order.customer.name</code>, <code>items[2].price</code>) through fields and array slots. Method calls, constructors, assignments and <code>++</code> are refused before they reach the program, with or without approval. Variables named like secrets (password, token, secret, apiKey, credential…) are masked. Results are capped: frames, variables and long values are cut and say so.
</p>

<h2>The session belongs to the conversation</h2>
<p>
  The session outlives one message: when a run ends the program stays alive (suspended, if it was stopped at a breakpoint, so you can inspect it) and the next message goes on from there. There is one debug session per conversation. It ends when Imber detaches (a program it launched is stopped, one it attached to is resumed and left running), when the program exits, after the idle time in Settings › Imber › Debug session idle time (30 minutes by default, counted from the last debug tool), when the project closes, or when you press <em>Stop</em> beside the debugger controls, where the line "Imber … ends in N min if unused" shows how long is left. Breakpoints it set without starting a session stay for you to use.
</p>

<h2>When the program stops while Imber is not waiting</h2>
<p>
  Bennu itself notices the stop and takes a snapshot while the program is still suspended: where it stopped, the top of the stack, the top frame's locals and the values the breakpoint asked to capture (capped and with secrets masked). What happens next follows the conversation:
</p>
<ul>
  <li><strong>A run is active</strong>: the snapshot reaches Imber as a system note with its next tool result.</li>
  <li><strong>No run is active</strong>: the note is queued for your next message, and the thread shows a card ("Stopped at Order.java:42 in OrderService.total") with <em>Send to Imber now</em>.</li>
  <li><strong>Start a run by itself</strong>: off by default (Settings › Imber › Start a run when the program stops, or per conversation from the composer's menu › Decisions). At most three an hour per conversation. It spends money and counts toward your limits.</li>
</ul>

<h2>Changing the program: a separate switch</h2>
<p>
  <em>Let it change the program it debugs</em> (off by default) offers four tools: <code>debug_throw</code> creates an exception inside the program (its constructor runs there) and throws it from the line the stopped thread is on, <code>debug_force_return</code> makes the method on top of the stack return a literal value at once (finally blocks included), <code>debug_drop_frame</code> discards a frame so the call runs again, <code>debug_set_value</code> writes a number, boolean, character, String or null to a local, a field or a path such as <code>order.total</code> (final and static fields are refused). All need a stop; the last two need a VM that supports them. The program changes while it runs and <strong>there is no undo</strong>: side effects already produced stay. Imber asks once per conversation (Autonomous does not skip the question) and every change is listed in the thread and in the Debug panel as an <em>Imber action</em>.
</p>
<p>
  The debugger tools target Java programs: a main class to launch, or a JVM listening on a local debug port.
</p>
