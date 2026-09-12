<script lang="ts">
  /**
   * Keyboard shortcuts: every binding, by area. The canonical list is keybindings.ts; this page follows it.
   */
  import Callout from '$lib/components/shared/ui/Callout.svelte';
</script>

<span class="eyebrow">Reference</span>
<h1>Keyboard shortcuts</h1>

<p class="doc-lead">
  Bennu is keyboard-first: every action below is reachable without the mouse, and <kbd>Ctrl</kbd> + <kbd>K</kbd> finds any of them by name.
</p>
<Callout variant="info" title="Why not Ctrl + Alt + a letter">
  Chromium drops those chords on Italian, German, French and Spanish layouts to keep AltGr working — so where IntelliJ uses one, Bennu uses <kbd>Alt</kbd> + <kbd>Shift</kbd> or another key,
  and the table says which.
</Callout>

<h2>Tool windows</h2>
<table>
  <thead><tr><th>Shortcut</th><th>Toggles</th></tr></thead>
  <tbody>
    <tr><td><kbd>Alt</kbd> + <kbd>1</kbd></td><td><strong>Project</strong> — files</td></tr>
    <tr><td><kbd>Alt</kbd> + <kbd>2</kbd></td><td><strong>Structure</strong> — symbols</td></tr>
    <tr><td><kbd>Alt</kbd> + <kbd>N</kbd></td><td><strong>Dependencies</strong> — by module, or by crate on a Cargo project</td></tr>
    <tr><td><kbd>Alt</kbd> + <kbd>8</kbd></td><td>The <strong>build tool</strong> window, right rail — Maven's goals on Java, Cargo's crates and commands on Rust</td></tr>
    <tr><td><kbd>Alt</kbd> + <kbd>5</kbd></td><td><strong>Tests</strong> — the catalogue, right rail</td></tr>
    <tr><td><kbd>Alt</kbd> + <kbd>9</kbd></td><td><strong>Trees</strong> — the parse and the declaration model, right rail; Java and JSP, since both views read Bennu's own engines</td></tr>
    <tr><td><kbd>Alt</kbd> + <kbd>Shift</kbd> + <kbd>I</kbd></td><td><strong>i18n</strong> — the fulcrum translation under the caret, rendered. Only on a translation bundle, so there is no rail button</td></tr>
    <tr><td><kbd>Alt</kbd> + <kbd>0</kbd></td><td><strong>Build</strong>, bottom dock</td></tr>
    <tr><td><kbd>Alt</kbd> + <kbd>R</kbd></td><td><strong>Run</strong> — the launched program's console, and the debugger when it is one</td></tr>
    <tr><td><kbd>Alt</kbd> + <kbd>6</kbd></td><td><strong>Problems</strong></td></tr>
    <tr><td><kbd>Alt</kbd> + <kbd>7</kbd></td><td><strong>TODO</strong></td></tr>
    <tr><td><kbd>Alt</kbd> + <kbd>3</kbd></td><td><strong>Forms</strong> — JSP form analysis</td></tr>
    <tr><td><kbd>Alt</kbd> + <kbd>F12</kbd></td><td><strong>Terminal</strong></td></tr>
    <tr><td><kbd>Alt</kbd> + <kbd>Shift</kbd> + <kbd>D</kbd></td><td>The <strong>module graph</strong> window; inside it <kbd>Alt</kbd> + <kbd>S</kbd> isolates the selected module's world. <kbd>Ctrl</kbd> + <kbd>Shift</kbd> + <kbd>G</kbd> is <em>Switch window</em>, bound everywhere</td></tr>
    <tr><td><kbd>Alt</kbd> + <kbd>Shift</kbd> + <kbd>U</kbd></td><td><strong>Download dependencies</strong> the poms need and the local repository lacks, then rebuild the index — a background job, Maven only</td></tr>
    <tr><td><kbd>Ctrl</kbd> + <kbd>Shift</kbd> + <kbd>M</kbd></td><td><strong>Structural search &amp; replace</strong></td></tr>
  </tbody>
</table>

<h2>Navigate</h2>
<table>
  <thead><tr><th>Shortcut</th><th>Goes to</th></tr></thead>
  <tbody>
    <tr><td><kbd>Ctrl</kbd> + <kbd>N</kbd></td><td>A <strong>class</strong> by name, project-wide — <strong>Types</strong> on a Rust project, answered by the language server</td></tr>
    <tr><td><kbd>Ctrl</kbd> + <kbd>Shift</kbd> + <kbd>N</kbd></td><td>A <strong>file</strong> by name</td></tr>
    <tr><td><kbd>Ctrl</kbd> + <kbd>Shift</kbd> + <kbd>Y</kbd></td><td>A <strong>symbol</strong> — methods and fields; functions and constants on Rust</td></tr>
    <tr><td><kbd>Tab</kbd>, in the navigator</td><td>The next of <strong>All · Classes · Files · Symbols</strong></td></tr>
    <tr><td><kbd>Ctrl</kbd> + <kbd>Alt</kbd> + <kbd>←</kbd> / <kbd>→</kbd></td><td><strong>Back</strong> / <strong>forward</strong> through the places you jumped to. Actions make a stop; moving the caret does not</td></tr>
    <tr><td><kbd>Ctrl</kbd> + <kbd>Shift</kbd> + <kbd>Backspace</kbd></td><td>The <strong>last place you edited</strong>; again for the one before</td></tr>
    <tr><td><kbd>Ctrl</kbd> + <kbd>Shift</kbd> + <kbd>E</kbd></td><td><strong>Recent locations</strong>, filterable, with the line you were on and a toggle for edited places</td></tr>
    <tr><td><kbd>Ctrl</kbd> + <kbd>F12</kbd></td><td><strong>File structure</strong> — a searchable outline; from the server on a language-server file</td></tr>
    <tr><td><kbd>Ctrl</kbd> + <kbd>H</kbd></td><td>The <strong>type hierarchy</strong> of the type at the caret — anywhere inside it</td></tr>
    <tr><td><kbd>Ctrl</kbd> + <kbd>Shift</kbd> + <kbd>H</kbd></td><td>The <strong>call hierarchy</strong> of the method at the caret — one level at a time</td></tr>
    <tr><td><kbd>Ctrl</kbd> + <kbd>G</kbd></td><td>A line, or line:column</td></tr>
    <tr><td><kbd>Ctrl</kbd> + <kbd>B</kbd>, <kbd>Ctrl</kbd> + click</td><td>The <strong>declaration</strong> of the symbol under the caret — its usages when already on it</td></tr>
    <tr><td><kbd>Alt</kbd> + <kbd>F7</kbd></td><td>Its <strong>usages</strong>, across the project</td></tr>
    <tr><td><kbd>Alt</kbd> + <kbd>Shift</kbd> + <kbd>F7</kbd></td><td>The usages of <strong>this component</strong>, in a <code>.svelte</code> file</td></tr>
    <tr><td><kbd>Ctrl</kbd> + <kbd>F</kbd> / <kbd>Ctrl</kbd> + <kbd>Shift</kbd> + <kbd>F</kbd></td><td>Find in the file / in the project</td></tr>
  </tbody>
</table>

<h2>Edit</h2>
<table>
  <thead><tr><th>Shortcut</th><th>Does</th></tr></thead>
  <tbody>
    <tr><td><kbd>Ctrl</kbd> + <kbd>Space</kbd>, <kbd>Ctrl</kbd> + <kbd>Shift</kbd> + <kbd>Space</kbd> — on macOS <kbd>Cmd</kbd> + <kbd>Shift</kbd> + <kbd>Space</kbd></td><td><strong>Completions</strong> at the caret; member access also opens on <kbd>.</kbd>, and a command language like <code>.dev</code> after a space. When nothing is found the footer says so. macOS takes the whole Control + Space family above the application</td></tr>
    <tr><td><kbd>Alt</kbd> + <kbd>Enter</kbd> — <kbd>Option</kbd> + <kbd>Enter</kbd> on a Mac</td><td><strong>Intentions</strong> at the caret — Bennu's quick fixes on Java, the server's code actions on a file it owns</td></tr>
    <tr><td><kbd>Ctrl</kbd> / <kbd>Cmd</kbd> + <kbd>Enter</kbd></td><td>A <strong>blank line below</strong>, caret on it — not intentions, one modifier away on a Mac</td></tr>
    <tr><td><kbd>Shift</kbd> + <kbd>F6</kbd></td><td><strong>Rename</strong> the symbol under the caret, with a preview</td></tr>
    <tr><td><kbd>Alt</kbd> + <kbd>Delete</kbd></td><td><strong>Safe delete</strong> the member under the caret — only when nothing uses it; otherwise every use is listed</td></tr>
    <tr><td><kbd>Alt</kbd> + <kbd>Insert</kbd></td><td><strong>Generate</strong> a constructor, getters or setters</td></tr>
    <tr><td><kbd>Ctrl</kbd> / <kbd>Cmd</kbd> + <kbd>I</kbd></td><td><strong>Implement / override methods</strong>, grouped by declaring type, abstract ones ticked</td></tr>
    <tr><td><kbd>Alt</kbd> + <kbd>Shift</kbd> + <kbd>F</kbd></td><td><strong>Format</strong> the file — by its server (<code>rustfmt</code> for Rust), or Bennu's formatter for Java</td></tr>
    <tr><td><kbd>Ctrl</kbd> + <kbd>Shift</kbd> + <kbd>O</kbd></td><td><strong>Optimize imports</strong> in a Java file — Eclipse's key, since IntelliJ's <kbd>Ctrl</kbd> + <kbd>Alt</kbd> + <kbd>O</kbd> is dropped on those layouts</td></tr>
    <tr><td><kbd>Alt</kbd> + <kbd>Shift</kbd> + <kbd>→</kbd> / <kbd>←</kbd></td><td><strong>Expand</strong> / <strong>shrink</strong> the selection by one syntactic step — VS Code's keys, since <kbd>Ctrl</kbd> + <kbd>W</kbd> may close a WebView</td></tr>
    <tr><td><kbd>Tab</kbd> / <kbd>Shift</kbd> + <kbd>Tab</kbd></td><td>Walk a completion's <strong>placeholders</strong>; <kbd>Esc</kbd> leaves</td></tr>
    <tr><td><kbd>Alt</kbd> + <kbd>Shift</kbd> + <kbd>M</kbd></td><td><strong>Expand the macro</strong> at the caret — recursive, and text rather than a file</td></tr>
    <tr><td><kbd>Ctrl</kbd> + <kbd>/</kbd></td><td>Toggle a <strong>comment</strong> — Java <code>//</code>, JSP <code>&lt;%-- --%&gt;</code>, XML <code>&lt;!-- --&gt;</code></td></tr>
    <tr><td><kbd>Ctrl</kbd> + <kbd>Z</kbd> / <kbd>Ctrl</kbd> + <kbd>Shift</kbd> + <kbd>Z</kbd></td><td><strong>Undo</strong> / <strong>redo</strong> — the history is the buffer's, so undo never reaches past the file in front of you</td></tr>
    <tr><td><kbd>Ctrl</kbd> + <kbd>Y</kbd> — on a Mac also <kbd>Cmd</kbd> + <kbd>⌫</kbd></td><td>Delete the current line, wherever the caret is in it</td></tr>
    <tr><td><kbd>Ctrl</kbd> + <kbd>S</kbd></td><td><strong>Save</strong> the current file</td></tr>
    <tr><td><kbd>Alt</kbd> + <kbd>Shift</kbd> + <kbd>H</kbd></td><td><strong>Local history</strong> of the open file; inside it <kbd>Alt</kbd> + <kbd>D</kbd> switches side by side and unified</td></tr>
  </tbody>
</table>
<p>
  Brackets and quotes close themselves — typing <kbd>(</kbd> <kbd>[</kbd> <kbd>&#123;</kbd> <kbd>"</kbd> <kbd>'</kbd> inserts the closer, and <kbd>Backspace</kbd> on an empty pair removes both.
</p>

<h2>Files</h2>
<table>
  <thead><tr><th>Shortcut</th><th>In the Project tree</th></tr></thead>
  <tbody>
    <tr><td><kbd>F2</kbd></td><td><strong>Rename</strong> the file — on Rust also its <code>mod</code> declaration and <code>use</code> paths, with the count shown first</td></tr>
    <tr><td><kbd>Ctrl</kbd> + <kbd>C</kbd></td><td><strong>Copy</strong> the file itself — <em>Copy path</em> in the menu copies the text</td></tr>
    <tr><td><kbd>Ctrl</kbd> + <kbd>V</kbd></td><td><strong>Paste</strong> it into the folder or package you are on — a Java copy gets its package, type and imports rewritten</td></tr>
    <tr><td><kbd>Del</kbd> / <kbd>⌫</kbd></td><td><strong>Delete</strong> the focused file or folder — it asks first, and keeps it in local history</td></tr>
    <tr><td><kbd>Ctrl</kbd> / <kbd>Cmd</kbd> + <kbd>Z</kbd></td><td><strong>Undo the last delete</strong> — with the tree focused; in the editor it undoes typing</td></tr>
  </tbody>
</table>

<h2>Build, run &amp; debug</h2>
<table>
  <thead><tr><th>Shortcut</th><th>Does</th></tr></thead>
  <tbody>
    <tr><td><kbd>Ctrl</kbd> + <kbd>F9</kbd></td><td><strong>Build</strong> — the type chosen on the Build split button: a Maven compile, or a validation without compiling; <code>cargo check</code> on Cargo</td></tr>
    <tr><td><kbd>Shift</kbd> + <kbd>F10</kbd></td><td><strong>Run</strong> the active configuration — a JVM one builds first; a Cargo one is its own build</td></tr>
    <tr><td><kbd>Ctrl</kbd> + <kbd>Shift</kbd> + <kbd>F10</kbd></td><td><strong>Run what is in front of you</strong> — the test at the caret, or a hot-swap deploy of the current JSP</td></tr>
    <tr><td><kbd>Shift</kbd> + <kbd>F9</kbd></td><td><strong>Debug</strong> the active configuration</td></tr>
    <tr><td><kbd>Ctrl</kbd> + <kbd>F8</kbd></td><td>Set or clear a <strong>breakpoint</strong> on the caret's line</td></tr>
    <tr><td><kbd>Ctrl</kbd> + <kbd>Shift</kbd> + <kbd>F8</kbd></td><td>The <strong>breakpoint list</strong> — disable, remove, add an exception breakpoint</td></tr>
    <tr><td><kbd>F9</kbd></td><td><strong>Resume</strong> the stopped program</td></tr>
    <tr><td><kbd>F8</kbd> / <kbd>F7</kbd> / <kbd>Shift</kbd> + <kbd>F8</kbd></td><td>Step <strong>over</strong> / <strong>into</strong> / <strong>out</strong></td></tr>
  </tbody>
</table>
<p>
  <kbd>F9</kbd> is <em>build</em> with <kbd>Ctrl</kbd> and <em>resume</em> without it, and they never collide: resume only exists while the program is stopped. The Build split button's choice —
  <em>Maven build</em> or <em>Validate (no compile)</em> — is remembered and becomes what <kbd>Ctrl</kbd> + <kbd>F9</kbd> runs; see <strong>Building &amp; running</strong>.
</p>

<h2>Tests</h2>
<table>
  <thead><tr><th>Shortcut</th><th>Runs</th></tr></thead>
  <tbody>
    <tr><td><kbd>Ctrl</kbd> + <kbd>Shift</kbd> + <kbd>F10</kbd></td><td>The <strong>test at the caret</strong> — above the first test, the Java class or the Rust file's target</td></tr>
    <tr><td><kbd>Ctrl</kbd> + <kbd>Shift</kbd> + <kbd>F5</kbd></td><td><strong>Every test</strong></td></tr>
    <tr><td><kbd>Ctrl</kbd> + <kbd>F5</kbd></td><td>Exactly what ran last</td></tr>
    <tr><td>Command palette</td><td><strong>Rerun failed tests</strong>, <strong>Stop the test run</strong></td></tr>
  </tbody>
</table>
<p>
  In a test run's tree: <kbd>↑</kbd> <kbd>↓</kbd> move, <kbd>→</kbd> <kbd>←</kbd> expand and collapse, <kbd>Enter</kbd> jumps to the source, <kbd>Ctrl</kbd> + <kbd>Enter</kbd> runs what the
  cursor is on.
</p>

<h2>Frameworks</h2>
<table>
  <thead><tr><th>Shortcut</th><th>Opens</th></tr></thead>
  <tbody>
    <tr><td><kbd>Alt</kbd> + <kbd>Shift</kbd> + <kbd>J</kbd></td><td>The <strong>DTO Lab</strong> on the class at the caret, or toggles its panel elsewhere; inside, <kbd>Ctrl</kbd> + <kbd>Enter</kbd> runs the payload or applies the tests</td></tr>
    <tr><td><kbd>Alt</kbd> + <kbd>4</kbd></td><td><strong>Endpoints</strong> — request mappings and Struts actions in one list</td></tr>
    <tr><td><kbd>Ctrl</kbd> + <kbd>Shift</kbd> + <kbd>B</kbd></td><td><strong>Spring beans</strong></td></tr>
    <tr><td><kbd>Alt</kbd> + <kbd>Shift</kbd> + <kbd>B</kbd></td><td><strong>Bevy components</strong>, each expanding into the systems reading and writing it</td></tr>
    <tr><td>Command palette</td><td><strong>Message bundles</strong>, <strong>Spring configuration</strong>, <strong>Spring bound properties</strong></td></tr>
  </tbody>
</table>
<Callout variant="info" title="Only where there is something to show">
  A service with no routes gets no Endpoints button, shortcut or palette entry, rather than a door onto an empty panel. Endpoints and Bevy components have a rail button because you keep them open
  while working; the rest open from the palette, so the rail does not grow a row per framework.
</Callout>

<h2>Window</h2>
<table>
  <thead><tr><th>Shortcut</th><th>Does</th></tr></thead>
  <tbody>
    <tr><td><kbd>Ctrl</kbd> + <kbd>K</kbd></td><td>Command palette</td></tr>
    <tr><td><kbd>Ctrl</kbd> + <kbd>O</kbd></td><td>Open project</td></tr>
    <tr><td><kbd>Ctrl</kbd> + <kbd>Shift</kbd> + <kbd>W</kbd></td><td>Manage workspaces</td></tr>
    <tr><td><kbd>Ctrl</kbd> + <kbd>,</kbd></td><td>Settings</td></tr>
    <tr><td><kbd>F1</kbd></td><td>Documentation</td></tr>
  </tbody>
</table>
<p>
  The palette also carries the plugin host: <strong>Plugin Manager</strong>, <strong>Plugin Marketplace</strong>, <strong>Toggle Plugin Logs</strong> and <strong>Reload plugins</strong> — which
  reads the plugin directories again without restarting Bennu, the loop you are in while writing one. What a plugin registers with <code>arbor.command.register</code> appears under
  <strong>Plugin Commands</strong>, and each view a plugin contributes gets an <strong>Open View: …</strong> entry.
</p>
