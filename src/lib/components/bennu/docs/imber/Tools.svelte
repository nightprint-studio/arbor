<span class="eyebrow">Imber</span>
<h1>Imber: Tools it can use</h1>

<p class="doc-lead">
  Imber acts only through a fixed set of tools, grouped in families; each family can be switched off in Settings, and a family that is off is not even offered to the model.
</p>

<h2>The families</h2>
<table>
  <thead><tr><th>Family (Settings label)</th><th>Tools</th><th>What it is for</th></tr></thead>
  <tbody>
    <tr><td><strong>Read and search files</strong></td><td><code>search_text</code>, <code>find_files</code>, <code>bennu_read_file</code>, <code>read_files</code>, <code>file_outline</code></td><td>Read a file (or several in one call), see a file's declarations with their line ranges, search text, find files by name.</td></tr>
    <tr><td><strong>Project intelligence</strong></td><td><code>bennu_find_symbol</code>, <code>bennu_find_usages</code>, <code>bennu_references</code>, <code>bennu_callers</code>, <code>bennu_symbol_at</code>, <code>bennu_outline</code>, <code>bennu_check_file</code>, <code>bennu_problems</code>, <code>bennu_catalog</code>, <code>bennu_frameworks</code>, <code>bennu_module_graph</code>, <code>bennu_project_summary</code>, <code>bennu_tests</code>, <code>bennu_todos</code> and more</td><td>Bennu's own knowledge of the project: symbols, usages, beans, endpoints, framework catalogues, problems, tests, TODOs, the module graph, the profiler summary.</td></tr>
    <tr><td><strong>Edit files</strong></td><td><code>edit_file</code>, <code>edit_files</code>, <code>replace_in_files</code>, <code>create_file</code>, <code>delete_file</code></td><td>Exact edits (one or a list across files, all or nothing), text or regular-expression replace over a folder with a dry run and a cap on files, create, delete.</td></tr>
    <tr><td><strong>Refactoring</strong></td><td><code>rename_symbol</code>, <code>add_parameter</code></td><td>Rename a symbol or add a parameter across every file that mentions it, from the project index, as one change. A renamed public type's file is renamed with it.</td></tr>
    <tr><td><strong>Structural search and replace</strong></td><td><code>structural_replace</code></td><td>Rewrite code by its shape, safer than a regular expression.</td></tr>
    <tr><td><strong>Local History</strong></td><td><code>local_history</code></td><td>List a file's revisions, show a diff, list deleted files, restore one.</td></tr>
    <tr><td><strong>Build, run and test</strong></td><td><code>bennu_build</code>, <code>bennu_test_run</code>, <code>bennu_cargo_run</code>, <code>bennu_npm_run_script</code>, run configurations, launching a program</td><td>Anything that executes the project. See Imber › Permissions and modes for when each asks.</td></tr>
    <tr><td><strong>Live services</strong></td><td><code>run_start</code>, <code>run_status</code>, <code>run_logs</code>, <code>run_stop</code>, <code>wait_ready</code>, <code>list_recipes</code>, <code>run_recipe</code>, <code>http_request</code>, <code>trace_request</code>, <code>timeline</code>, <code>list_scenarios</code>, <code>run_scenario</code>, <code>save_scenario</code>, <code>console_logs</code>, <code>tail_file</code>, <code>wait_log</code>, <code>log_facets</code>, <code>log_record</code></td><td>Start the project's services, read their logs, call them over localhost, follow one request through every log, run scenarios. See Imber › Running and observing.</td></tr>
    <tr><td><strong>Debugger</strong></td><td><code>debug_set_breakpoint</code>, <code>debug_list_breakpoints</code>, <code>debug_remove_breakpoint</code>, <code>debug_launch</code>, <code>debug_attach</code>, <code>debug_wait_stop</code>, <code>debug_stack</code>, <code>debug_variables</code>, <code>debug_evaluate</code>, <code>debug_step</code>, <code>debug_resume</code>, <code>debug_detach</code>, <code>debug_output</code>, <code>debug_run_to</code>, <code>debug_break_on_exception</code>, <code>debug_select_thread</code></td><td>Debug a Java program. See Imber › Debugging with Imber.</td></tr>
    <tr><td><strong>Look into a running Bevy game</strong></td><td><code>brp_connect</code>, <code>brp_query</code>, <code>brp_get</code>, <code>brp_resources</code>, <code>brp_get_resource</code>, <code>brp_types</code></td><td>Read entities, components, resources and types over the Bevy Remote Protocol. Only for projects with a <code>Cargo.toml</code>. See Imber › Bevy games and the profiler.</td></tr>
    <tr><td><strong>Screenshots of a page</strong></td><td><code>screenshot_page</code>, <code>page_diagnostics</code></td><td>Look at a front end on localhost. See Imber › Screenshots and diagrams.</td></tr>
  </tbody>
</table>

<h2>Switches that are not in the family list</h2>
<ul>
  <li><strong>Let it change a running Bevy game</strong> (<code>brp_mutate</code>, <code>brp_insert</code>, <code>brp_remove</code>, <code>brp_spawn</code>, <code>brp_despawn</code>, <code>brp_set_resource</code>): off by default, needs the Bevy family on.</li>
  <li><strong>Let it change the program it debugs</strong> (<code>debug_throw</code>, <code>debug_force_return</code>, <code>debug_drop_frame</code>, <code>debug_set_value</code>): off by default, needs the Debugger family on.</li>
  <li><strong>Git</strong> (<code>corvus_status</code>, <code>corvus_diff</code>, <code>corvus_log</code>, <code>corvus_blame</code>, <code>corvus_stage</code>, <code>corvus_unstage</code>, <code>corvus_commit</code>): off by default. See below.</li>
  <li><strong>Sub-agents</strong> (<code>spawn_agent</code>, <code>await_agents</code>): off by default; and the peer tools <code>file_owner</code> and <code>request_change</code> for talking to other conversations about a file. See Imber › Sub-agents.</li>
  <li><strong>Step plan</strong> (<code>update_plan</code>): on by default.</li>
</ul>

<h2>Tools that are always there</h2>
<p>
  <code>ask_user</code> (questions, Imber › Decisions and questions), <code>bennu_help</code> (the help about Bennu itself), <code>open_ui</code> (opens a panel or window), <code>show_diagram</code> (draws a Mermaid diagram), <code>load_skill</code> (only when you enabled a skill, Imber › Skills) and <code>expand_result</code> (returns the detail of a digest). In a chat only <code>bennu_help</code>, <code>open_ui</code> and <code>show_diagram</code> are offered.
</p>

<h2>Switching a family off</h2>
<p>
  Settings › Imber › Tools it may use has one switch per family. A family that is off is not offered at all: Imber cannot call it, its definitions cost nothing, and a call that slips through is refused with "the user has switched that kind of tool off". "Do not touch my files" is <em>Edit files</em> off; "do not run anything" is <em>Build, run and test</em> and <em>Live services</em> off. The switches are the defaults for new conversations and for the next message of the open one.
</p>

<h2>Help about Bennu</h2>
<p>
  Ask how to do something in Bennu (a feature, a shortcut, where a panel is) and Imber looks it up in Bennu's own documentation, this manual, and in a map of the window's buttons and shortcuts, instead of answering from memory. If the documentation has nothing, it says so. It works in a chat too, and so does opening a panel: ask to see the dependencies, the problems or the tests and Imber opens them; in a chat that needs no access to your project.
</p>

<h2>Git</h2>
<p>
  Off by default; Settings › Imber › Git turns it on. Imber can then read the repository (status, diff, log, blame) and stage, unstage and commit through Corvus's own git tools. A commit always asks first, in every mode, and it cannot push. Git is not available in a chat.
</p>
<p>
  Reading means commit messages and author names go to the provider. <strong>Hide authors</strong> (on by default) shows the model each person as a made-up id such as <code>Author-7f3a9c</code>: the same person is always the same id, so "who wrote most of this" still works. The ids are turned back into names in what you read and in anything Imber sends to git, so a commit message crediting an id credits the real person. The mapping stays on this machine. Git also enables the "Git worktree" kind of isolated copy.
</p>

<h2>Several projects</h2>
<p>
  With the reach set to <em>All open projects</em> Imber works across the projects open in the workspace, the one you are in first, up to <em>Projects at once</em> (6 by default; each is a line in its instructions, so a higher number costs a little on every request). It reads, searches and changes in all of them: a tool takes a project by name (or <code>name:path</code>), the text search and the file finder take <code>project: "all"</code>, and a change goes to the project that owns the file, which keeps its own Local History. A proposal names a file in another project as <code>name:path</code>.
</p>

<h2>Big answers are digests</h2>
<p>
  A build with sixty errors, a test run with forty failures, a long search or a long list of usages is answered with a digest (counts, distinct problems grouped, the files most affected and what was left out); <code>expand_result</code> returns the detail for one file, test or message. A file Imber already read comes back as one line when unchanged, and as the lines that differ when it changed.
</p>
