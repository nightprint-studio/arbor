<!-- Bennu docs — Keyboard shortcuts. -->
<h1>Keyboard shortcuts</h1>
<p class="doc-lead">Bennu is keyboard-first — every action below is reachable without the mouse.</p>

<h2>Tool windows</h2>
<ul>
  <li><kbd>Alt</kbd> + <kbd>1</kbd> — toggle <strong>Project</strong> (files)</li>
  <li><kbd>Alt</kbd> + <kbd>2</kbd> — toggle <strong>Structure</strong> (symbols)</li>
  <li><kbd>Alt</kbd> + <kbd>N</kbd> — toggle <strong>Dependencies</strong> (by module, or by crate on a Cargo project)</li>
  <li><kbd>Alt</kbd> + <kbd>Shift</kbd> + <kbd>D</kbd> — the <strong>module graph</strong> — who depends on whom inside the project, in a window. Inside it, <kbd>Alt</kbd> + <kbd>S</kbd> isolates the selected module's world. <kbd>Ctrl</kbd> + <kbd>Shift</kbd> + <kbd>G</kbd> would have been the obvious key for the window itself and is <em>Switch window</em>, bound in every window</li>
  <li><kbd>Alt</kbd> + <kbd>0</kbd> — toggle <strong>Build</strong> (bottom dock)</li>
  <li><kbd>Alt</kbd> + <kbd>R</kbd> — toggle <strong>Run</strong> — the launched program's console, and the debugger when it is one (bottom dock)</li>
  <li><kbd>Alt</kbd> + <kbd>5</kbd> — toggle <strong>Tests</strong> (the catalogue, right rail)</li>
  <li><kbd>Alt</kbd> + <kbd>9</kbd> — toggle <strong>Trees</strong> — the parse and the declaration model (right rail). Java and JSP: both views read Bennu's own engines, so a Rust project has the Cargo window there instead</li>
  <li><kbd>Ctrl</kbd> + <kbd>Shift</kbd> + <kbd>M</kbd> — <strong>structural search &amp; replace</strong></li>
  <li><kbd>Alt</kbd> + <kbd>6</kbd> — toggle <strong>Problems</strong> (bottom dock)</li>
  <li><kbd>Alt</kbd> + <kbd>7</kbd> — toggle <strong>TODO</strong> (bottom dock)</li>
  <li><kbd>Alt</kbd> + <kbd>3</kbd> — toggle <strong>Forms</strong> — JSP form analysis (bottom dock)</li>
  <li><kbd>Alt</kbd> + <kbd>F12</kbd> — toggle <strong>Terminal</strong> (bottom dock)</li>
  <li><kbd>Alt</kbd> + <kbd>8</kbd> — toggle the <strong>build tool</strong> window (right rail): <strong>Maven</strong>'s goals on a Java project, <strong>Cargo</strong>'s crates and commands on a Rust one</li>
  <li><kbd>Alt</kbd> + <kbd>Shift</kbd> + <kbd>I</kbd> — toggle <strong>i18n</strong> (right rail) — the fulcrum translation under the caret, rendered, with its parameters and a language picker. Only on a translation bundle; there is no rail button, because on every other file the panel could only say "not here"</li>
</ul>

<h2>Navigate</h2>
<ul>
  <li><kbd>Ctrl</kbd> + <kbd>N</kbd> — go to <strong>class</strong> by name (project-wide). On a Rust project the tab reads <strong>Types</strong> and the language server answers — what it finds are structs, enums, traits and type aliases</li>
  <li><kbd>Ctrl</kbd> + <kbd>Shift</kbd> + <kbd>N</kbd> — go to <strong>file</strong> by name</li>
  <li><kbd>Ctrl</kbd> + <kbd>Shift</kbd> + <kbd>Y</kbd> — go to <strong>symbol</strong> (methods and fields, project-wide; functions and constants on a Rust project)</li>
  <li><kbd>Tab</kbd> — inside the navigator, move between <strong>All · Classes · Files · Symbols</strong></li>
  <li><kbd>Ctrl</kbd> + <kbd>Alt</kbd> + <kbd>←</kbd> / <kbd>Ctrl</kbd> + <kbd>Alt</kbd> + <kbd>→</kbd> — navigate <strong>back</strong> / <strong>forward</strong> through your recent jumps (across files)</li>
  <li><kbd>Ctrl</kbd> + <kbd>F12</kbd> — <strong>file structure</strong>: a searchable outline of the file in front of you. On a language-server file the server supplies it, so a Rust file lists its structs, traits, impls and functions</li>
  <li><kbd>Ctrl</kbd> + <kbd>H</kbd> — <strong>type hierarchy</strong> of the type at the caret: its implementors, or by direction what it is built on. The caret may be anywhere inside the type — on a member it climbs to the owner</li>
  <li><kbd>Ctrl</kbd> + <kbd>Shift</kbd> + <kbd>H</kbd> — <strong>call hierarchy</strong> of the method at the caret: its callers, or by direction what it calls. Both open the Hierarchy panel, which expands one level at a time and takes the keyboard as it opens — a recursive chain is walked as far as you care to and no further</li>
</ul>

<h2>Editor</h2>
<ul>
  <li><kbd>Ctrl</kbd> + <kbd>G</kbd> — go to line (or line:column)</li>
  <li><kbd>Ctrl</kbd> + <kbd>B</kbd> (or <kbd>Ctrl</kbd> + click) — go to declaration of the symbol under the caret (or show its usages when already on the declaration)</li>
  <li><kbd>Alt</kbd> + <kbd>F7</kbd> — <strong>find usages</strong> of the symbol under the caret (across the project)</li>
  <li><kbd>Alt</kbd> + <kbd>Shift</kbd> + <kbd>F7</kbd> — <strong>find usages of this component</strong>, in a <code>.svelte</code> file: the file <em>is</em> the component, so there is no name inside it to put the caret on</li>
  <li><kbd>Del</kbd> / <kbd>⌫</kbd> — in the Project tree, <strong>delete</strong> the focused file or folder (it asks first)</li>
  <li><kbd>Ctrl</kbd>/<kbd>Cmd</kbd> + <kbd>Z</kbd> — with the Project tree focused, <strong>undo the last delete</strong>. In the editor the same keys undo typing: two stacks, and whichever has focus answers</li>
  <li><kbd>Alt</kbd> + <kbd>Shift</kbd> + <kbd>H</kbd> — <strong>local history</strong> of the open file. IntelliJ's own binding is <kbd>Ctrl</kbd> + <kbd>Alt</kbd> + a letter, which Chromium drops on Italian, German, French and Spanish layouts to keep AltGr working</li>
  <li><kbd>Alt</kbd> + <kbd>D</kbd> — inside that window, switch its comparison between <strong>side by side</strong> and the unified patch</li>
  <li><kbd>Ctrl</kbd> + <kbd>Z</kbd> / <kbd>Ctrl</kbd> + <kbd>Shift</kbd> + <kbd>Z</kbd> — <strong>undo</strong> / <strong>redo</strong>. The history belongs to the buffer you are in: opening another file starts a fresh one, so undo can never reach past the file in front of you</li>
  <li><kbd>Ctrl</kbd> + <kbd>Y</kbd> — on a Mac also <kbd>Cmd</kbd> + <kbd>⌫</kbd> — delete the current line, whole, wherever in it the caret is (IntelliJ's keys; redo is on <kbd>Ctrl</kbd> + <kbd>Shift</kbd> + <kbd>Z</kbd>)</li>
  <li><kbd>Ctrl</kbd> + <kbd>S</kbd> — <strong>save</strong> the current file</li>
  <li><kbd>Ctrl</kbd> + <kbd>F</kbd> — find in the current file</li>
  <li><kbd>Ctrl</kbd> + <kbd>Shift</kbd> + <kbd>F</kbd> — find in project</li>
  <li><kbd>Ctrl</kbd> + <kbd>Space</kbd> or <kbd>Ctrl</kbd> + <kbd>Shift</kbd> + <kbd>Space</kbd> —
    on macOS <kbd>Cmd</kbd> + <kbd>Shift</kbd> + <kbd>Space</kbd> — <strong>completions</strong> at
    the caret. Member access also opens on <kbd>.</kbd>, and a command language such as
    <code>.dev</code> asks after a space too. When the request finds nothing, the footer says so for
    a moment, naming the position it asked at. <strong>The Mac spelling is not a preference</strong>:
    macOS takes the whole Control + Space family for switching input source, above the application,
    so no program on that system receives those two</li>
  <li><kbd>Ctrl</kbd> + <kbd>/</kbd> — toggle line / block <strong>comment</strong> on the selection (Java <code>//</code>, JSP <code>&lt;%-- --%&gt;</code>, XML <code>&lt;!-- --&gt;</code>)</li>
  <li>Auto-close brackets and quotes — typing <kbd>(</kbd> <kbd>[</kbd> <kbd>&#123;</kbd> <kbd>"</kbd> <kbd>'</kbd> inserts the matching closer; <kbd>Backspace</kbd> on an empty pair removes both</li>
  <li><kbd>Alt</kbd> + <kbd>Enter</kbd> (<kbd>Option</kbd> + <kbd>Enter</kbd> on a Mac) — show
    <strong>intentions</strong> at the caret. One list from whichever engine can answer: Bennu's own
    quick fixes on a <code>.java</code>, the language server's code actions on a file it owns —
    "import <code>HashMap</code>", "fill match arms", "add missing lifetime"</li>
  <li><kbd>Ctrl</kbd> / <kbd>Cmd</kbd> + <kbd>Enter</kbd> — open a <strong>blank line below</strong>
    and put the caret on it, wherever in the line you were. Not intentions — that is the key above,
    and on a Mac the two are one modifier apart</li>
  <li><kbd>Shift</kbd> + <kbd>F6</kbd> — <strong>rename</strong> the symbol under the caret (with a per-file preview)</li>
  <li><kbd>Alt</kbd> + <kbd>Insert</kbd> — <strong>generate</strong> constructor, getters or setters</li>
  <li><kbd>Ctrl</kbd> / <kbd>Cmd</kbd> + <kbd>I</kbd> — <strong>implement / override methods</strong>: everything the class inherits and may override, grouped by the type that declares it, with the abstract ones already ticked</li>
  <li><kbd>Alt</kbd> + <kbd>Shift</kbd> + <kbd>F</kbd> — <strong>format</strong> the file. A language with a <strong>language server</strong> is formatted by it (<code>rustfmt</code> for Rust, honouring the project's configuration); <strong>Java</strong> is re-indented and tidied by Bennu's own formatter</li>
  <li><kbd>Ctrl</kbd> + <kbd>Shift</kbd> + <kbd>O</kbd> — <strong>optimize imports</strong> in a Java file: drop what it does not use, order the rest. Eclipse's own key, because IntelliJ's <kbd>Ctrl</kbd> + <kbd>Alt</kbd> + <kbd>O</kbd> is dropped by Chromium on Italian, German, French and Spanish layouts to keep AltGr working</li>
  <li><kbd>Alt</kbd> + <kbd>Shift</kbd> + <kbd>→</kbd> / <kbd>Alt</kbd> + <kbd>Shift</kbd> + <kbd>←</kbd> — <strong>expand</strong> / <strong>shrink</strong> the selection by one syntactic step — the token, the expression, the argument list, the call, the block, on the server's own idea of structure. VS Code's keys rather than IntelliJ's <kbd>Ctrl</kbd> + <kbd>W</kbd>, which a WebView may read as "close the window"</li>
  <li><kbd>Tab</kbd> / <kbd>Shift</kbd> + <kbd>Tab</kbd> — after accepting a completion that inserted <strong>placeholders</strong>, move between them; <kbd>Esc</kbd> leaves the run. Accepting <code>println!</code> puts the caret between the parentheses, and a function completion walks you through its arguments</li>
  <li><kbd>Alt</kbd> + <kbd>Shift</kbd> + <kbd>M</kbd> — <strong>expand the macro</strong> at the caret and read what it generates (also under <kbd>Alt</kbd> + <kbd>Enter</kbd>). The expansion is recursive (all the way down — the server has no single-step form) and is text rather than a file, so it cannot be navigated: to go deeper, point at the nested macro in the source and expand again</li>
</ul>

<h2>Files</h2>
<ul>
  <li><kbd>F2</kbd> — <strong>rename</strong> the file selected in the Project tree. On a Rust project the rename also rewrites the <code>mod</code> declaration that names the file and every <code>use</code> path through it; the dialog says how many files it will touch before you commit to it</li>
</ul>

<h2>Build &amp; run</h2>
<ul>
  <li><kbd>Ctrl</kbd> + <kbd>F9</kbd> — <strong>build</strong> the project — runs the type chosen on the Build split-button: a Maven compile (<code>javac</code> fallback), or a whole-project <strong>validation without compiling</strong>. In a Cargo project it runs <code>cargo check</code> over the workspace</li>
  <li><kbd>Shift</kbd> + <kbd>F10</kbd> — <strong>run</strong> the active configuration. A JVM one builds first, then launches; a Cargo one is a cargo subcommand, which is its own build</li>
  <li><kbd>Ctrl</kbd> + <kbd>Shift</kbd> + <kbd>F10</kbd> — <strong>run what is in front of you</strong>: the test at the caret in a Java or Rust test file, or a hot-swap deploy of the current JSP</li>
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
  <li><kbd>Ctrl</kbd> + <kbd>Shift</kbd> + <kbd>F10</kbd> — run the <strong>test at the caret</strong>: the method or <code>fn</code> you are inside, or — with the caret above the first test — the whole class in Java and the file's cargo target in Rust</li>
  <li><kbd>Ctrl</kbd> + <kbd>Shift</kbd> + <kbd>F5</kbd> — run <strong>every test</strong>: the project under Maven, the whole workspace under cargo</li>
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
  <li><kbd>Alt</kbd> + <kbd>4</kbd> — <strong>Endpoints</strong> — every URL the application answers, request mappings and Struts actions in one list, each expanding into what it takes and what it returns</li>
  <li><kbd>Ctrl</kbd> + <kbd>Shift</kbd> + <kbd>B</kbd> — <strong>Spring beans</strong> — every bean the project declares</li>
  <li><kbd>Alt</kbd> + <kbd>Shift</kbd> + <kbd>B</kbd> — <strong>Bevy components</strong> — every component, resource, message, event and bundle, each expanding into the systems that read and write it</li>
  <li>Command palette — <strong>Message bundles</strong> (every key with its text, how many places read it, and which locales are missing it), <strong>Spring configuration</strong> (every property key, and which <code>application*.yml</code> resolves first) and <strong>Spring bound properties</strong> (every <code>@ConfigurationProperties</code> field with the key it binds)</li>
</ul>
<p>
  These appear only on a project that actually uses the framework <em>and</em> has something in the
  list — a service with no routes of either kind gets no Endpoints button, shortcut or palette
  entry, rather than a door onto an empty panel. Endpoints and Bevy components have a button in
  the right activity bar, because both are lists you keep open while working; the others are
  opened from the palette, so the rail doesn't grow a row per framework.
</p>

<h2>Window</h2>
<ul>
  <li><kbd>Ctrl</kbd> + <kbd>K</kbd> — command palette</li>
  <li><kbd>Ctrl</kbd> + <kbd>O</kbd> — open project</li>
  <li><kbd>Ctrl</kbd> + <kbd>Shift</kbd> + <kbd>W</kbd> — manage workspaces</li>
  <li><kbd>Ctrl</kbd> + <kbd>,</kbd> — settings</li>
  <li><kbd>F1</kbd> — documentation</li>
</ul>
<p>
  The palette also carries the plugin host: <strong>Plugin Manager</strong>,
  <strong>Plugin Marketplace</strong>, <strong>Toggle Plugin Logs</strong> and
  <strong>Reload plugins</strong> — which re-reads the plugin directories without restarting
  Bennu, the loop you are in while writing one. Anything a loaded plugin registered with
  <code>arbor.command.register</code> appears under <strong>Plugin Commands</strong>, and each
  view a plugin contributes gets its own <strong>Open View: …</strong> entry.
</p>
