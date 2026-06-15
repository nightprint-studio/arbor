<!-- Nemus docs — Projects & tabs. Plain semantic HTML; DocsShell supplies the typography. -->
<h1>Projects &amp; tabs</h1>
<p class="doc-lead">
  A nemus <strong>project</strong> is just a folder: an optional <code>nemus.toml</code> manifest
  and one or more <code>.nemus</code> files (found by walking the folder). Everything — the
  arrangement, mixer, libraries — reads from those files; there's no hidden session state.
</p>

<h2>Opening &amp; creating</h2>
<ul>
  <li><strong>New project</strong> (<kbd>Ctrl</kbd> + <kbd>Shift</kbd> + <kbd>N</kbd>) — pick a folder; nemus writes a <code>nemus.toml</code> and a starter <code>.nemus</code>, and opens it.</li>
  <li><strong>Open project</strong> (<kbd>Ctrl</kbd> + <kbd>O</kbd>) — choose an existing project folder. A folder without a manifest is still a valid project (its name is the folder name).</li>
  <li><strong>Recent projects</strong> — the title bar lists the ones you've opened before; the last project reopens automatically on launch.</li>
  <li><strong>Open file</strong> (<kbd>Ctrl</kbd> + <kbd>Shift</kbd> + <kbd>O</kbd>) — add a <code>.nemus</code> to the tab strip.</li>
</ul>

<h2>The manifest</h2>
<pre><code># nemus.toml
name = "Suzuran Town"
audience = "for the festival set"

[libraries]                       # external GitHub modules (see Libraries)
drums = "github:owner/repo@v1"</code></pre>
<p>
  Every field is optional. <code>name</code> labels the project (rename it from the title-bar
  menu, which rewrites this key); <code>audience</code> is a free "for whom" note. The
  <code>[libraries]</code> table declares shared modules. Render defaults (sample rate, bit depth,
  reverb tail) live in <strong>Settings</strong>, not here.
</p>

<h2>Files &amp; tabs</h2>
<ul>
  <li>The <strong>Files</strong> panel lists the project's <code>.nemus</code> files with a per-file summary; click one to open it in a tab.</li>
  <li>The editor's <strong>tab strip</strong> holds your open files. Switch with a click — the arrangement, mixer and lint follow the visible file; <strong>drag</strong> a tab to reorder; the <strong>×</strong> closes it; <kbd>Ctrl</kbd> + <kbd>N</kbd> (or the <kbd>+</kbd>) starts a new file.</li>
  <li><strong>Outline</strong> mirrors the active file's tracks, functions and constants — click a symbol to jump, <kbd>Ctrl</kbd> + click a name in the editor to follow it (cross-file too).</li>
</ul>

<h2>Workspaces</h2>
<p>
  Group related projects into named, colour-coded <strong>workspaces</strong> — switch between
  them from the project dropdown in the title bar, or manage them with <strong>Manage
  workspaces…</strong> (the dropdown or the Command Palette). When a workspace is active, the
  dropdown lists its member projects for one-click swapping; with none active it falls back to
  your recents. Add the open project to a workspace from the manage dialog. Workspaces are saved
  with the rest of the nemus window state.
</p>

<h2>Picking up where you left off</h2>
<p>
  Your open tabs are remembered <strong>per project</strong> (in the project's <code>.nemus/</code>
  folder) and restored when you reopen it. <strong>Scratch</strong> tabs persist globally, so your
  experiments come back too. Reopening a project — or relaunching nemus — lands you exactly where
  you were.
</p>

<div class="callout">
  Local imports are project-relative: <code>import &lbrace; kick &rbrace; from "lib/drums.nemus"</code>
  pulls from a file in the same folder — keep shared helpers in a <code>lib/</code> subfolder.
  The <code>$lib/</code> prefix is reserved for declared GitHub libraries.
</div>
