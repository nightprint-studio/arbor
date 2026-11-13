<span class="eyebrow">Imber</span>
<h1>Imber: Settings reference</h1>

<p class="doc-lead">
  Every Imber setting, in the order of the page Settings › Imber: what it does, its default, the key it is stored under, and whether a conversation can override it.
</p>

<h2>Where settings are stored</h2>
<ul>
  <li>All settings below are in the <code>[assistant]</code> table of Bennu's config file for the active profile, <code>arbor/profiles/&lt;profile&gt;/bennu/config.toml</code> (the limits in <code>[assistant.limits]</code>). The key is the name in the "Key" column.</li>
  <li>The <strong>API key</strong> is never in a config file: it is in a credential vault of its own.</li>
  <li>The <strong>price list</strong> is <code>assistant-models.toml</code> and your <strong>own instructions</strong> are <code>assistant-instructions.md</code>, both in the same Bennu config folder. Your own skills go in its <code>skills</code> folder, or in any folder you add under Tools and skills.</li>
  <li>Conversations, the spending ledger (<code>usage.jsonl</code>) and the git author mapping live in the data folder <code>arbor/data/bennu/assistant</code>. Isolated copies live in <code>assistant-worktrees</code> beside it.</li>
</ul>

<h2>Connection</h2>
<table>
  <thead><tr><th>Setting</th><th>What it does</th><th>Default</th><th>Key</th><th>Per conversation</th></tr></thead>
  <tbody>
    <tr><td><strong>API key</strong></td><td>Your Anthropic key. Save, or Remove.</td><td>Not set</td><td>(vault)</td><td>No</td></tr>
    <tr><td><strong>Admin key (optional)</strong></td><td>An Anthropic Admin key, read only by the credit meter in the title bar to show what your organization really spent this month; never sent with a conversation. Only Anthropic organizations have Admin keys.</td><td>Not set</td><td>(vault)</td><td>No</td></tr>
  </tbody>
</table>

<h2>Models and prices</h2>
<table>
  <thead><tr><th>Setting</th><th>What it does</th><th>Default</th><th>Key</th><th>Per conversation</th></tr></thead>
  <tbody>
    <tr><td><strong>Price list</strong></td><td>Create a price file, a copy you can edit to change a price or add a model; Reload applies it. The path is shown below the buttons. A file that cannot be read is reported and the built-in list is used.</td><td>Built-in list</td><td>(file)</td><td>No</td></tr>
  </tbody>
</table>

<h2>Git</h2>
<table>
  <thead><tr><th>Setting</th><th>What it does</th><th>Default</th><th>Key</th><th>Per conversation</th></tr></thead>
  <tbody>
    <tr><td><strong>Let Imber use git</strong></td><td>Status, diff, log, blame, stage, unstage and commit through Corvus. Not available in a chat.</td><td>Off</td><td><code>git_enabled</code></td><td>No</td></tr>
    <tr><td><strong>Hide authors</strong></td><td>Shows the model each person as a made-up id; turned back into names for you. Needs git on.</td><td>On</td><td><code>hide_authors</code></td><td>No</td></tr>
  </tbody>
</table>

<h2>Spending limits</h2>
<table>
  <thead><tr><th>Setting</th><th>What it does</th><th>Default</th><th>Key</th><th>Per conversation</th></tr></thead>
  <tbody>
    <tr><td><strong>Per conversation</strong></td><td>The most one conversation may cost, in euros. 0 = off.</td><td>5</td><td><code>limits.session_eur</code></td><td>No</td></tr>
    <tr><td><strong>Per day</strong></td><td>From midnight, your time.</td><td>10</td><td><code>limits.day_eur</code></td><td>No</td></tr>
    <tr><td><strong>Per week</strong></td><td>From Monday.</td><td>0 (off)</td><td><code>limits.week_eur</code></td><td>No</td></tr>
    <tr><td><strong>Per month</strong></td><td>From the first.</td><td>50</td><td><code>limits.month_eur</code></td><td>No</td></tr>
    <tr><td><strong>Exchange rate</strong></td><td>Euros per dollar, kept by hand.</td><td>0.92</td><td><code>limits.eur_per_usd</code></td><td>No</td></tr>
    <tr><td><strong>Warn from</strong></td><td>The composer flags a limit at this percentage of it (10 to 99).</td><td>80</td><td><code>limits.warn_percent</code></td><td>No</td></tr>
    <tr><td><strong>Scheduled run cap (EUR)</strong></td><td>The most one scheduled run may cost, in euros; it ends as incomplete when it gets there. 0 = no cap.</td><td>1</td><td><code>limits.schedule_run_eur</code></td><td>No</td></tr>
    <tr><td><strong>Scheduled runs per day</strong></td><td>How many scheduled runs may start in a day. 0 = no cap.</td><td>24</td><td><code>limits.schedule_runs_per_day</code></td><td>No</td></tr>
    <tr><td><strong>Credit balance</strong></td><td>The account's balance as the Claude Console shows it, in dollars, entered by hand (the API does not report it). Saving records the moment too: the title bar's meter estimates what is left from what is spent after it. Update, or Clear.</td><td>Not entered</td><td><code>limits.credit_usd</code>, <code>limits.credit_set_at</code></td><td>No</td></tr>
  </tbody>
</table>
<p>Each limit field also shows how much has been spent against it.</p>

<h2>New conversations</h2>
<table>
  <thead><tr><th>Setting</th><th>What it does</th><th>Default</th><th>Key</th><th>Per conversation</th></tr></thead>
  <tbody>
    <tr><td><strong>Model</strong></td><td>What a new conversation starts with.</td><td>Opus 5.5</td><td><code>model</code></td><td>Yes, composer</td></tr>
    <tr><td><strong>Largest context window</strong></td><td>The most Imber may put in one conversation.</td><td>200K</td><td><code>max_context</code></td><td>Yes, composer</td></tr>
    <tr><td><strong>Effort</strong></td><td>How hard it thinks (Low to Max); disabled for a model with no effort setting.</td><td>High</td><td><code>effort</code></td><td>Yes, composer</td></tr>
    <tr><td><strong>Steps per run</strong></td><td>Requests to the model one run may make before it stops with “Stopped after N steps” and waits for you to say <em>continue</em> (5 to 300). A step is one request, however many tools it calls. Also applies to Imber in Corvus.</td><td>40</td><td><code>max_steps</code></td><td>No</td></tr>
    <tr><td><strong>Cheaper model for side jobs</strong></td><td>Writes compaction summaries and runs read-only sub-agents. Empty means the cheapest Haiku in the price list.</td><td>Default (cheapest Haiku)</td><td><code>side_model</code></td><td>No</td></tr>
    <tr><td><strong>Keep instructions cached for an hour</strong></td><td>Use the provider's one-hour cache for the instructions and tools.</td><td>On</td><td><code>long_cache</code></td><td>No</td></tr>
    <tr><td><strong>Step plan</strong></td><td>A visible plan with a percentage for multi-step work.</td><td>On</td><td><code>plan_enabled</code></td><td>Yes, composer</td></tr>
    <tr><td><strong>Sub-agents</strong></td><td>Offer clean-context helpers.</td><td>Off</td><td><code>subagents_enabled</code></td><td>Yes: follow, on or off</td></tr>
    <tr><td><strong>Wrap pasted code in a fenced block</strong></td><td>Pasted code becomes a fenced block with its language.</td><td>On</td><td><code>wrap_pasted_code</code></td><td>No (use Ctrl+Shift+V once)</td></tr>
    <tr><td><strong>Project instructions</strong></td><td>Read <code>CLAUDE.md</code>, <code>AGENTS.md</code> and your own instructions file.</td><td>On</td><td><code>use_instructions</code></td><td>No</td></tr>
    <tr><td><strong>Chat only</strong></td><td>New conversations are chats with no tools.</td><td>Off</td><td><code>chat_only</code></td><td>Chosen when the conversation starts (Agent / Chat switch)</td></tr>
    <tr><td><strong>Mode</strong></td><td>Manual, Accept edits or Autonomous.</td><td>Manual</td><td><code>mode</code></td><td>Yes, composer or palette</td></tr>
    <tr><td><strong>Decisions</strong></td><td>Ask at forks, Only big decisions, Decide itself.</td><td>Ask at forks</td><td><code>decision_level</code></td><td>Yes</td></tr>
    <tr><td><strong>Your instructions about decisions</strong></td><td>Free text, up to 2,000 characters.</td><td>Empty</td><td><code>decision_notes</code></td><td>Yes: its own text is appended</td></tr>
    <tr><td><strong>General context</strong></td><td>Standing notes about you (role, stack, tone, language, rules) added to Imber's instructions in every conversation, agent or chat, inside a <code>&lt;user_context&gt;</code> block. Written as a list, one note per item (<kbd>Enter</kbd> adds the next, <kbd>Alt</kbd> + <kbd>↑</kbd> / <kbd>↓</kbd> moves one); up to 4,000 characters in all. Edit it from this page, the panel's ⋯ menu or the command palette (<em>Imber: General context…</em>); Ctrl/Cmd+Enter saves. The same modal has a second tab, <em>This project</em>: notes about the open repository (conventions, what not to touch), kept in <code>.arbor/bennu/config.toml</code> under <code>[assistant]</code> and sent in a <code>&lt;project_context&gt;</code> block only with conversations about it, under the same cap. What a whole team should read belongs in <code>CLAUDE.md</code>.</td><td>Empty</td><td><code>general_context</code></td><td>No</td></tr>
    <tr><td><strong>Debug session idle time</strong></td><td>Minutes before an unused debug session is ended (5 to 240).</td><td>30</td><td><code>debug_idle_minutes</code></td><td>No</td></tr>
    <tr><td><strong>Notifications › When it waits for you</strong></td><td>A system notification, and the Arbor icon asking for attention, when Imber stops on a question, an approval or a card while no Bennu window is in front.</td><td>On</td><td><code>notify_questions</code></td><td>No</td></tr>
    <tr><td><strong>Notifications › When a run ends</strong></td><td>The same when a run finishes or stops on an error while no Bennu window is in front.</td><td>Off</td><td><code>notify_done</code></td><td>No</td></tr>
    <tr><td><strong>Confirm costly changes</strong></td><td>Ask before a model or window change that makes a long conversation re-read its history at full price.</td><td>On</td><td><code>confirm_cost_changes</code></td><td>No</td></tr>
  </tbody>
</table>

<h2>What it may reach</h2>
<table>
  <thead><tr><th>Setting</th><th>What it does</th><th>Default</th><th>Key</th><th>Per conversation</th></tr></thead>
  <tbody>
    <tr><td><strong>Reach</strong></td><td>This file, this project, all open projects, anywhere on disk.</td><td>This project</td><td><code>scope</code> (<code>file</code>, <code>project</code>, <code>workspace</code>, <code>disk</code>)</td><td>Yes, composer</td></tr>
    <tr><td><strong>Projects at once</strong></td><td>With the reach "all open projects": how many projects one conversation works across (1 to 20). Projects Imber brings in count too.</td><td>6</td><td><code>max_projects</code></td><td>No</td></tr>
    <tr><td><strong>Opening other projects</strong></td><td>When Imber needs Bennu on a folder that is not open in Bennu (indexed where it can be: a Java build, or a language server) but that it may already read: <em>Ask, except in Autonomous</em> (<code>autonomous</code>), <em>Always ask</em> (<code>ask</code>), <em>Open where it may read</em> (<code>allowed</code>) or <em>Never</em> (<code>never</code>). A project open in Bennu joins without this question; one outside the reach is asked about with the access card. See Imber › Tools, Several projects.</td><td>Ask, except in Autonomous</td><td><code>open_projects</code></td><td>No</td></tr>
    <tr><td><strong>Edit files</strong></td><td>Off makes Imber read-only.</td><td>On</td><td><code>allow_edits</code></td><td>Yes, composer › Permissions</td></tr>
    <tr><td><strong>Use the Temp folders</strong></td><td>Scratch space outside the project.</td><td>On</td><td><code>allow_temp</code></td><td>Yes, composer › Permissions</td></tr>
    <tr><td><strong>Folders it may work in, besides the project</strong></td><td>Extra folders allowed in any reach. Add folder… opens the folder picker.</td><td>None</td><td><code>extra_folders</code></td><td>No</td></tr>
  </tbody>
</table>

<h2>Tools it may use</h2>
<table>
  <thead><tr><th>Setting</th><th>What it does</th><th>Default</th><th>Key</th><th>Per conversation</th></tr></thead>
  <tbody>
    <tr><td><strong>One switch per family</strong>: Read and search files, Project intelligence, Edit files, Refactoring, Structural search and replace, Local History, Build run and test, Debugger, Look into a running Bevy game, Screenshots of a page, Live services</td><td>A family that is off is not offered at all.</td><td>All on</td><td><code>tool_families_off</code> (a list of the families switched off)</td><td>No</td></tr>
    <tr><td><strong>Programs it may run</strong></td><td>Programs on this machine Imber may call by name with <code>run_tool</code>. Each has a name, a description Imber reads, the program's absolute path (<em>Choose…</em> opens the explorer), fixed arguments (one per line) and variables (<code>KEY=value</code> per line), whether Imber may add arguments, which variables it may set, whether it changes things, when a call asks (every call, once per conversation, never), a time limit in seconds and an on/off switch. A tool the run would not offer is marked with the reason. In the Build run and test family.</td><td>None</td><td><code>external_tools</code></td><td>No</td></tr>
    <tr><td><strong>Let it change a running Bevy game</strong></td><td>Mutate, insert, remove, spawn, despawn, set a resource. Needs the Bevy family.</td><td>Off</td><td><code>bevy_write_enabled</code></td><td>No</td></tr>
    <tr><td><strong>Let it change the program it debugs</strong></td><td>Throw, return early, drop a frame, write a value. Needs the Debugger family.</td><td>Off</td><td><code>debug_mutate_enabled</code></td><td>No</td></tr>
    <tr><td><strong>Start a run when the program stops</strong></td><td>When the debugged program stops and Imber is idle, start a run (at most 3 an hour per conversation).</td><td>Off</td><td><code>auto_continue_on_stop</code></td><td>Yes, composer › Decisions</td></tr>
  </tbody>
</table>

<h2>Skills</h2>
<table>
  <thead><tr><th>Setting</th><th>What it does</th><th>Default</th><th>Key</th><th>Per conversation</th></tr></thead>
  <tbody>
    <tr><td><strong>Skills…</strong></td><td>Opens the skills manager and shows "N of M enabled".</td><td>None enabled</td><td><code>skills_on</code></td><td>No</td></tr>
  </tbody>
</table>

<h2>Settings that live only in the composer or the panel</h2>
<ul>
  <li><strong>Isolated copy</strong> (off, copy, git worktree) and <strong>Build isolation</strong> (shared queue or separate target): chosen per conversation, fixed once it starts.</li>
  <li><strong>Dry run</strong>: Send shows the request instead of making it.</li>
</ul>
