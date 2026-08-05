<!-- Bennu docs — Keyboard shortcuts. -->
<h1>Keyboard shortcuts</h1>
<p class="doc-lead">Bennu is keyboard-first — every action below is reachable without the mouse.</p>

<h2>Tool windows</h2>
<ul>
  <li><kbd>Alt</kbd> + <kbd>1</kbd> — toggle <strong>Project</strong> (files)</li>
  <li><kbd>Alt</kbd> + <kbd>2</kbd> — toggle <strong>Structure</strong> (symbols)</li>
  <li><kbd>Alt</kbd> + <kbd>N</kbd> — toggle <strong>Dependencies</strong> (by module)</li>
  <li><kbd>Alt</kbd> + <kbd>0</kbd> — toggle <strong>Build</strong> (bottom dock)</li>
  <li><kbd>Alt</kbd> + <kbd>R</kbd> — toggle <strong>Run</strong> — the launched program's console, and the debugger when it is one (bottom dock)</li>
  <li><kbd>Alt</kbd> + <kbd>5</kbd> — toggle <strong>Tests</strong> (the catalogue, right rail)</li>
  <li><kbd>Alt</kbd> + <kbd>6</kbd> — toggle <strong>Problems</strong> (bottom dock)</li>
  <li><kbd>Alt</kbd> + <kbd>7</kbd> — toggle <strong>TODO</strong> (bottom dock)</li>
  <li><kbd>Alt</kbd> + <kbd>3</kbd> — toggle <strong>Forms</strong> — JSP form analysis (bottom dock)</li>
  <li><kbd>Alt</kbd> + <kbd>F12</kbd> — toggle <strong>Terminal</strong> (bottom dock)</li>
  <li><kbd>Alt</kbd> + <kbd>8</kbd> — toggle <strong>Maven</strong> (right rail)</li>
</ul>

<h2>Navigate</h2>
<ul>
  <li><kbd>Ctrl</kbd> + <kbd>N</kbd> — go to <strong>class</strong> by name (project-wide)</li>
  <li><kbd>Ctrl</kbd> + <kbd>Shift</kbd> + <kbd>N</kbd> — go to <strong>file</strong> by name</li>
  <li><kbd>Ctrl</kbd> + <kbd>Shift</kbd> + <kbd>Y</kbd> — go to <strong>symbol</strong> (methods and fields, project-wide)</li>
  <li><kbd>Tab</kbd> — inside the navigator, move between <strong>All · Classes · Files · Symbols</strong></li>
  <li><kbd>Ctrl</kbd> + <kbd>Alt</kbd> + <kbd>←</kbd> / <kbd>Ctrl</kbd> + <kbd>Alt</kbd> + <kbd>→</kbd> — navigate <strong>back</strong> / <strong>forward</strong> through your recent jumps (across files)</li>
</ul>

<h2>Editor</h2>
<ul>
  <li><kbd>Ctrl</kbd> + <kbd>G</kbd> — go to line (or line:column)</li>
  <li><kbd>Ctrl</kbd> + <kbd>B</kbd> (or <kbd>Ctrl</kbd> + click) — go to declaration of the symbol under the caret (or show its usages when already on the declaration)</li>
  <li><kbd>Alt</kbd> + <kbd>F7</kbd> — <strong>find usages</strong> of the symbol under the caret (across the project)</li>
  <li><kbd>Ctrl</kbd> + <kbd>S</kbd> — <strong>save</strong> the current file</li>
  <li><kbd>Ctrl</kbd> + <kbd>F</kbd> — find in the current file</li>
  <li><kbd>Ctrl</kbd> + <kbd>Shift</kbd> + <kbd>F</kbd> — find in project</li>
  <li><kbd>Ctrl</kbd> + <kbd>Space</kbd> — completions (member access on <kbd>.</kbd>)</li>
  <li><kbd>Ctrl</kbd> + <kbd>/</kbd> — toggle line / block <strong>comment</strong> on the selection (Java <code>//</code>, JSP <code>&lt;%-- --%&gt;</code>, XML <code>&lt;!-- --&gt;</code>)</li>
  <li>Auto-close brackets and quotes — typing <kbd>(</kbd> <kbd>[</kbd> <kbd>&#123;</kbd> <kbd>"</kbd> <kbd>'</kbd> inserts the matching closer; <kbd>Backspace</kbd> on an empty pair removes both</li>
  <li><kbd>Alt</kbd> + <kbd>Enter</kbd> — show <strong>intentions</strong> (context actions) at the caret</li>
  <li><kbd>Shift</kbd> + <kbd>F6</kbd> — <strong>rename</strong> the symbol under the caret (with a per-file preview)</li>
  <li><kbd>Alt</kbd> + <kbd>Insert</kbd> — <strong>generate</strong> constructor, getters or setters</li>
</ul>

<h2>Build &amp; run</h2>
<ul>
  <li><kbd>Ctrl</kbd> + <kbd>F9</kbd> — <strong>build</strong> the project — runs the type chosen on the Build split-button: a Maven compile (<code>javac</code> fallback), or a whole-project <strong>validation without compiling</strong>. In a Cargo project it runs <code>cargo check</code> over the workspace</li>
  <li><kbd>Shift</kbd> + <kbd>F10</kbd> — <strong>run</strong> the active configuration (builds first, then launches) — Maven projects only</li>
  <li><kbd>Ctrl</kbd> + <kbd>Shift</kbd> + <kbd>F10</kbd> — <strong>run what is in front of you</strong>: the test at the caret in a Java test file, or a hot-swap deploy of the current JSP</li>
</ul>

<h2>Debug</h2>
<ul>
  <li><kbd>Shift</kbd> + <kbd>F9</kbd> — <strong>debug</strong> the active configuration (the same launch as ▶, with a debugger attached)</li>
  <li><kbd>Ctrl</kbd> + <kbd>F8</kbd> — set or clear a <strong>breakpoint</strong> on the caret's line</li>
  <li><kbd>Ctrl</kbd> + <kbd>Shift</kbd> + <kbd>F8</kbd> — the <strong>breakpoint list</strong>: disable, remove, or add an exception breakpoint</li>
  <li><kbd>F9</kbd> — <strong>resume</strong> the stopped program</li>
  <li><kbd>F8</kbd> — <strong>step over</strong></li>
  <li><kbd>F7</kbd> — <strong>step into</strong></li>
  <li><kbd>Shift</kbd> + <kbd>F8</kbd> — <strong>step out</strong></li>
</ul>
<p>
  <kbd>F9</kbd> is <em>build</em> with <kbd>Ctrl</kbd> and <em>resume</em> without it; they can
  never collide, because resume only exists while the program is stopped. Breakpoints can also be
  set by clicking the left margin, outside the line numbers, and right-clicking one offers to
  disable rather than delete it. See <em>Building &amp; running</em>.
</p>
<p>In a Maven project the <strong>Build</strong> button is a split-button: click it to run the current type, or open its chevron to pick <em>Maven build</em> or <em>Validate (no compile)</em> — the choice is remembered and becomes what <kbd>Ctrl</kbd> + <kbd>F9</kbd> runs. In a Cargo project there is one build and the button is plain. Validation checks every <code>.java</code> file with the editor's diagnostics, reports timing statistics (total, average, slowest file) in the Build tool window, and lists every problem it finds in the <strong>Problems</strong> panel, grouped by file. Only one build or validation runs at a time. Build output streams to the <strong>Build</strong> tool window; problems there are clickable and jump to the offending line. A launched program's own output goes to the <strong>Run</strong> console instead, where you can type back to it and stop it. What ▷ launches is the active run configuration (▷ menu → <em>Edit configurations…</em>); on a project with exactly one entry point there is nothing to configure — ▷ finds it. See <em>Building &amp; running</em>.</p>

<h2>Tests</h2>
<ul>
  <li><kbd>Ctrl</kbd> + <kbd>Shift</kbd> + <kbd>F10</kbd> — run the <strong>test at the caret</strong>: the method you are inside, or the whole class when the caret is above the first test</li>
  <li><kbd>Ctrl</kbd> + <kbd>Shift</kbd> + <kbd>F5</kbd> — run <strong>every test</strong> in the project</li>
  <li><kbd>Ctrl</kbd> + <kbd>F5</kbd> — <strong>rerun</strong> exactly what ran last</li>
  <li>Command palette — <strong>Rerun failed tests</strong>, and <strong>Stop the test run</strong></li>
</ul>
<p>
  Inside a test run the tree is driven from the keyboard: <kbd>↑</kbd> <kbd>↓</kbd> move,
  <kbd>→</kbd> <kbd>←</kbd> expand and collapse a class, <kbd>Enter</kbd> jumps to the source and
  <kbd>Ctrl</kbd> + <kbd>Enter</kbd> runs whatever the cursor is on. See
  <em>Testing</em> for what the panel shows.
</p>

<h2>Frameworks</h2>
<ul>
  <li><kbd>Alt</kbd> + <kbd>4</kbd> — <strong>Endpoints</strong> — every request mapping, grouped, with its parameters</li>
  <li><kbd>Ctrl</kbd> + <kbd>Shift</kbd> + <kbd>B</kbd> — <strong>Spring beans</strong> — every bean the project declares</li>
  <li>Command palette — <strong>Spring configuration</strong> (every property key, and which <code>application*.yml</code> resolves first) and <strong>Spring bound properties</strong> (every <code>@ConfigurationProperties</code> field with the key it binds)</li>
</ul>
<p>
  These appear only on a project that actually uses the framework <em>and</em> has something in the
  list — a Spring service with no request mappings gets no Endpoints button, shortcut or palette
  entry, rather than a door onto an empty panel. Endpoints has a button in the right activity bar,
  because a route list is something you keep open; the others are opened from the palette, so the
  rail doesn't grow a row per framework.
</p>

<h2>Window</h2>
<ul>
  <li><kbd>Ctrl</kbd> + <kbd>K</kbd> — command palette</li>
  <li><kbd>Ctrl</kbd> + <kbd>O</kbd> — open project</li>
  <li><kbd>Ctrl</kbd> + <kbd>Shift</kbd> + <kbd>W</kbd> — manage workspaces</li>
  <li><kbd>Ctrl</kbd> + <kbd>,</kbd> — settings</li>
  <li><kbd>F1</kbd> — documentation</li>
</ul>
