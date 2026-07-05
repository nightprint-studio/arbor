<!-- Bennu docs — Keyboard shortcuts. -->
<h1>Keyboard shortcuts</h1>
<p class="doc-lead">Bennu is keyboard-first — every action below is reachable without the mouse.</p>

<h2>Tool windows</h2>
<ul>
  <li><kbd>Alt</kbd> + <kbd>1</kbd> — toggle <strong>Project</strong> (files)</li>
  <li><kbd>Alt</kbd> + <kbd>2</kbd> — toggle <strong>Structure</strong> (symbols)</li>
  <li><kbd>Alt</kbd> + <kbd>N</kbd> — toggle <strong>Dependencies</strong> (by module)</li>
  <li><kbd>Alt</kbd> + <kbd>0</kbd> — toggle <strong>Build</strong> (bottom dock)</li>
  <li><kbd>Alt</kbd> + <kbd>6</kbd> — toggle <strong>Problems</strong> (bottom dock)</li>
  <li><kbd>Alt</kbd> + <kbd>7</kbd> — toggle <strong>TODO</strong> (bottom dock)</li>
  <li><kbd>Alt</kbd> + <kbd>3</kbd> — toggle <strong>Forms</strong> — JSP form analysis (bottom dock)</li>
  <li><kbd>Alt</kbd> + <kbd>F12</kbd> — toggle <strong>Terminal</strong> (bottom dock)</li>
  <li><kbd>Alt</kbd> + <kbd>8</kbd> — toggle <strong>Maven</strong> (right rail)</li>
  <li><kbd>Alt</kbd> + <kbd>9</kbd> — toggle <strong>Services</strong> (right rail)</li>
</ul>

<h2>Navigate</h2>
<ul>
  <li><kbd>Ctrl</kbd> + <kbd>N</kbd> — go to <strong>class</strong> by name (project-wide)</li>
  <li><kbd>Ctrl</kbd> + <kbd>Shift</kbd> + <kbd>N</kbd> — go to <strong>file</strong> by name</li>
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
  <li><kbd>Ctrl</kbd> + <kbd>F9</kbd> — <strong>build</strong> the project — runs the type chosen on the Build split-button: a Maven compile (<code>javac</code> fallback), or a whole-project <strong>validation without compiling</strong></li>
  <li><kbd>Shift</kbd> + <kbd>F10</kbd> — <strong>run</strong> (builds first, then launches the main class)</li>
</ul>
<p>The <strong>Build</strong> button is a split-button: click it to run the current type, or open its chevron to pick <em>Maven build</em> or <em>Validate (no compile)</em> — the choice is remembered and becomes what <kbd>Ctrl</kbd> + <kbd>F9</kbd> runs. Validation checks every <code>.java</code> file with the editor's diagnostics, reports timing statistics (total, average, slowest file) in the Build tool window, and lists every problem it finds in the <strong>Problems</strong> panel, grouped by file. Only one build or validation runs at a time. Output streams to the <strong>Build</strong> tool window; problems there are clickable and jump to the offending line. The main class is set once per project from the run configuration (▷ menu → <em>Edit configurations…</em>).</p>

<h2>Window</h2>
<ul>
  <li><kbd>Ctrl</kbd> + <kbd>K</kbd> — command palette</li>
  <li><kbd>Ctrl</kbd> + <kbd>O</kbd> — open project</li>
  <li><kbd>Ctrl</kbd> + <kbd>Shift</kbd> + <kbd>W</kbd> — manage workspaces</li>
  <li><kbd>Ctrl</kbd> + <kbd>,</kbd> — settings</li>
  <li><kbd>F1</kbd> — documentation</li>
</ul>
