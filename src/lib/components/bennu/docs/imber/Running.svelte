<span class="eyebrow">Imber</span>
<h1>Imber: Running and observing</h1>

<p class="doc-lead">
  With the Live services family on, Imber can start the project's services, wait for them, read their logs economically, call them over localhost and run scenarios; builds from every conversation share one queue.
</p>

<p>
  The feature itself (processes, recipes, scenarios, the timeline, correlation ids) is described on the <strong>Live services</strong> page. This page is about how Imber uses it.
</p>

<h2>Managed processes</h2>
<ul>
  <li><code>run_start</code> starts a named process with a command, a working folder, optional ports and a readiness check (a port, a log line or a health URL). It asks every time, showing the command. The line runs in <code>bash</code> where the machine has it (<code>sh</code> otherwise), and its <code>cwd</code> may be any project of the conversation, by name (<code>voriz:</code>, <code>voriz:crates/app</code>) — the same for <code>cargo_run</code>, whose <code>cwd</code> also decides which Cargo workspace it runs in, and for <code>run_tool</code>. Tools such as <code>cargo</code> and <code>mvn</code> are found even when Arbor was started from the Finder: at start it adds the usual tool folders (<code>~/.cargo/bin</code>, Homebrew, SDKMAN) to the <code>PATH</code> every process inherits. Imber is told to prefer Bennu's own tools for a check, a build, the tests or <code>cargo run</code> (<code>bennu_check</code>, <code>bennu_build</code>, <code>bennu_test_run</code>, <code>cargo_run</code>), which need no card and answer with the parsed result; a command stays available when one of them cannot do the job.</li>
  <li><code>cargo_run</code> runs a binary or an example of a Rust project — package, <code>bin</code> or <code>example</code>, features, <code>release</code> or a profile, the program's own arguments and environment variables — as a managed process like any other. Imber gives fields, never a command line, and the command is the one a run configuration with the same fields would run, without a shell. The first run of a target (package and binary) in a conversation asks; the yes covers that target again with any features, arguments or environment, and another target asks again. A compile error shows in its logs. A server returns at once (its output is read with <code>run_logs</code>, also after it ends); a program that does its work and ends is run with <code>until_exit</code>, which waits for it (5 minutes by default, at most 30) and answers with its exit code and the last lines it printed, stderr marked — build errors included.</li>
  <li><code>run_status</code>, <code>run_logs</code>, <code>wait_ready</code> and <code>run_stop</code> inspect, wait for and stop what the conversation started. Reading never asks; stopping what the run started does not ask.</li>
  <li>The processes are the same ones the Live panel lists. Every process records its owner; a conversation stops only its own processes. A name or port that another conversation holds is refused instead of duplicated or killed, and a stack another conversation already runs is attached read-only.</li>
  <li>A yes given to a recipes file in one conversation does not carry to another.</li>
</ul>

<h2>Programs you set up</h2>
<p>
  <code>run_tool</code> runs one of the programs listed in Settings › Imber › Tools and skills › <em>Programs it may run</em>, by name. Each declaration sets the program (an absolute path), the arguments always passed, whether Imber may add its own, the variables always set and the ones Imber may set, a time limit, whether it changes things, and when a call asks: every call, once per conversation, or never. A tool that changes things asks at every call in Manual mode, whatever it says.
</p>
<ul>
  <li>The program runs directly, without a shell: each argument reaches it as written, so <code>;</code>, <code>|</code>, <code>$(…)</code> and wildcards are plain characters. An argument or a variable the declaration does not allow is refused before anything starts.</li>
  <li>It runs in a folder of the project (its root unless Imber names one) and Imber gets back the exit code with standard output and standard error; a long output keeps its beginning and its end. Imber can also send text to its standard input.</li>
  <li>A call that reaches its time limit, or a run you cancel, ends the program with its whole process tree.</li>
  <li>With <code>background</code> the program starts as a managed process instead — for a watcher or a server — and is read and stopped like any other (<code>run_logs</code>, <code>run_stop</code>).</li>
  <li>A yes given once per conversation lasts until you edit the tool. A scheduled run never uses these programs.</li>
  <li>The settings page marks a tool that would not be offered (no program at that path, a name that is not allowed, no description) and says why.</li>
</ul>

<h2>Scripts of the project</h2>
<p>
  <code>run_script</code> runs a script of the project that is not one of your programs — <code>.sh</code>, <code>.bash</code>, <code>.py</code>, <code>.js</code>, <code>.ps1</code>, <code>.bat</code>/<code>.cmd</code> on Windows, or an executable file. It always asks first, with a card that shows the script's path, the command, why Imber wants to run it and its first lines. Three answers:
</p>
<ul>
  <li><strong>Decline</strong> — nothing runs, and Imber is told.</li>
  <li><strong>Run once</strong> — this call only.</li>
  <li><strong>Run and add to my programs</strong> — it runs, and a declaration is added to Settings › Imber › Tools and skills › <em>Programs it may run</em> (interpreter, script, "changes things", asked once per conversation), for you to give a description and refine. From then on Imber calls it with <code>run_tool</code>.</li>
</ul>
<p>
  The script runs through its interpreter, chosen by its extension, or directly when it is executable; arguments are passed one by one, with no shell. It runs like a program you set up: the exit code and the output come back, it is stopped after ten minutes, and <code>background</code> makes it a managed process. Imber can edit the project's files, so a yes is for the script <em>as it is</em>: a script that changed asks again, and the card says it changed. The same holds for a program you set up whose file is inside the project — or a script you handed to an interpreter in its fixed arguments: its approval covers that content, and even one set to <em>Never</em> asks again when the file has changed since it last ran in the conversation.
</p>

<h2>Recipes and scenarios</h2>
<p>
  <code>list_recipes</code> and <code>run_recipe</code> use the project's recipes file (<code>.arbor/bennu/live/recipes.toml</code>); <code>list_scenarios</code>, <code>run_scenario</code> and <code>save_scenario</code> use scenario files. Running a recipe or a scenario asks once per <em>content</em> of its file: change the file and it asks again. A scenario is an ordered list of steps with assertions and also runs without Imber from the Live panel.
</p>

<h2>Requests and the timeline</h2>
<p>
  <code>http_request</code> reaches services on this machine and nothing else (<code>localhost</code>, <code>127.0.0.1</code>, <code>[::1]</code>). GET, HEAD and OPTIONS do not ask; any other method asks unless the port belongs to a process the conversation started. <code>trace_request</code> follows one request through the logs of every process by its correlation id, and <code>timeline</code> reads the unified timeline of requests, log events and process events.
</p>

<h2>Reading logs without reading them</h2>
<p>A log is read as <em>records</em> (a line with its stack trace, the <code>Caused by</code> chain or a backtrace), digest first, detail only when needed:</p>
<ul>
  <li><strong><code>log_facets</code></strong>: a compact digest instead of lines, for a process, the Run console or a log file: exceptions grouped by class and root cause, queries with literals removed, requests, warnings.</li>
  <li><strong><code>run_logs</code></strong>: a process's records with filters (level, pattern, time range, context) and a summary of the distinct errors.</li>
  <li><strong><code>log_record</code></strong>: one record in full, by the id a digest printed.</li>
  <li><strong><code>wait_log</code></strong>: waits for a new line matching a regular expression (default 30 s, at most 300), with context on each side.</li>
  <li><strong><code>console_logs</code></strong>: what the Run console printed for the Run button (the last six runs of a project).</li>
  <li><strong><code>tail_file</code></strong>: the end of a log file, whole records only; it returns an offset to read only what was appended next time. A read of a file follows the access rules.</li>
</ul>
<p>Every answer lists what it left out and how to ask for it. Details of the filters are on the Live services page.</p>

<h2>The live run card</h2>
<p>
  A run that used these tools ends with a <strong>Live run</strong> card: what it started and stopped, what is still running, the requests it sent (each opens the timeline on that request), the scenarios and assertions, the files it changed and what the run cost.
</p>

<h2>The build queue</h2>
<p>
  Builds and test runs take a numbered ticket in a queue shared by every conversation and by the Build button, and are served in order, one at a time per project. A waiting call simply returns its result when its turn has come; cancelling the run cancels the wait. Requests for the same module (or the same test selection) queued before a build starts are served by that one build, each with the same report and a note saying which state of the project it is about. The queue is listed in the Live panel.
</p>
<p>
  <strong>Build isolation</strong> (composer settings menu, fixed when the conversation starts): <em>Shared (queue)</em> is the default; <em>Separate target</em> makes a Cargo project compile in a target folder of its own, so the conversation neither waits nor makes others wait, at the cost of disk space and a full first compile. Other toolchains stay in the queue. A conversation in an isolated copy never queues against the project.
</p>

<h2>Who broke the build</h2>
<p>
  When a build fails, the report says which errors are in files this conversation edited, which are in files another conversation edited last, and which other conversations changed anything since the project last compiled. It is a guess from the edit record, because in Rust an error often shows in one file and comes from a change in another. Bennu leaves a short system note for each of those conversations: it reaches Imber with its next tool result, or with your next message if it is idle, lists the errors without repeating itself and is marked as a heuristic. A file nobody edited in this session is reported as yours.
</p>

<h2>Other conversations about a file</h2>
<p>
  Whether sub-agents are on or not, Imber can ask who owns a file (<code>file_owner</code>: the conversation that reserved or last changed it, whether it is running, its current plan step) and leave that conversation a note (<code>request_change</code>). The note is marked as coming from another agent, not from you, is delivered once, and a run may send only a few.
</p>
