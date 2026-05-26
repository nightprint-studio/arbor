<script lang="ts">
  import Callout from '$lib/components/shared/ui/Callout.svelte';
  import Kbd     from '$lib/components/shared/internal/Kbd.svelte';
</script>

<h1>Files</h1>

<p class="doc-lead">The <strong>Files</strong> panel shows every tracked file in the repository as a collapsible directory tree, with per-file last-commit metadata loaded progressively in the background.</p>

<h2>Opening the panel</h2>
<p>Click the <strong>Files</strong> icon in the Activity Bar (folder icon) to toggle the Files sidebar section.</p>

<h2>Tree navigation</h2>
<table class="shortcuts-table">
  <thead><tr><th>Action</th><th>How</th></tr></thead>
  <tbody>
    <tr><td>Expand / collapse a folder</td><td>Click the folder row or its chevron</td></tr>
    <tr><td>Filter graph by file</td><td>Click a file row (click again to clear)</td></tr>
    <tr><td>Context menu</td><td>Right-click any file row</td></tr>
    <tr><td>Search files</td><td>Type in the search box at the top of the panel</td></tr>
    <tr><td>Refresh</td><td>Click the <strong>↺</strong> refresh button in the panel toolbar</td></tr>
  </tbody>
</table>

<h2>File &amp; folder icons</h2>
<p>Icons are resolved using the <strong>VS Code Icons</strong> set (Iconify). Resolution order for files:</p>
<ol class="step-list">
  <li><strong>Exact filename match</strong> — e.g. <code>Cargo.toml</code>, <code>Dockerfile</code>, <code>package.json</code></li>
  <li><strong><code>.env*</code> prefix</strong> — any file starting with <code>.env</code> gets the dotenv icon</li>
  <li><strong><code>.d.ts</code> suffix</strong> — TypeScript definition files</li>
  <li><strong>Extension lookup</strong> — Rust, TypeScript, Svelte, Python, Go, Java, Kotlin, C/C++, and 30+ more</li>
  <li><strong>Fallback</strong> — plain text icon</li>
</ol>
<p>Folders are also matched by name: <code>src</code>, <code>components</code>, <code>node_modules</code>, <code>dist</code>, <code>test</code>, <code>docs</code>, <code>styles</code>, <code>types</code>, and many others resolve to semantic folder icons.</p>

<h2>Last-commit metadata</h2>
<p>Each file row shows a faint right-aligned column with:</p>
<ul>
  <li><strong>Short commit SHA</strong> — 7-character OID of the last commit that touched the file</li>
  <li><strong>Relative date</strong> — e.g. <em>today</em>, <em>3d ago</em>, <em>2mo ago</em></li>
  <li><strong>Commit summary</strong> — one-line commit message (truncated)</li>
</ul>
<p>
  Metadata is loaded <strong>lazily</strong>: the file list itself appears immediately (reading the git index is instant).
  The last-commit info is then streamed from a background Rust thread via batched Tauri events
  (<code>arbor://file-meta-batch</code>), so the tree remains usable while metadata fills in progressively.
</p>
<Callout variant="info" title="Session cache">
  Completed scans are saved to <code>sessionStorage</code> keyed by
  repository path + HEAD fingerprint. Re-opening the panel (or switching tabs and back) is instant
  as long as HEAD has not moved.
</Callout>

<h2>File search</h2>
<p>The search box filters files using a <strong>multi-tier fuzzy search</strong>:</p>
<table class="shortcuts-table">
  <thead><tr><th>Priority</th><th>Match type</th></tr></thead>
  <tbody>
    <tr><td>1 (highest)</td><td>Exact filename match</td></tr>
    <tr><td>2</td><td>Filename starts with query</td></tr>
    <tr><td>3</td><td>Filename contains query</td></tr>
    <tr><td>4</td><td>Full path contains query</td></tr>
    <tr><td>5</td><td>Fuzzy match on filename (characters appear in order)</td></tr>
    <tr><td>6</td><td>Fuzzy match on full path</td></tr>
  </tbody>
</table>
<p>Results are capped at 200 items. The search is debounced by 150 ms to avoid scoring on every keystroke.</p>
<Callout variant="info" title="Command Palette">
  The <em>Modified Files</em> section in the Command Palette (<Kbd action="command_palette" />)
  also searches the file tree and dispatches an <code>arbor:navigate-to-file</code> event that
  expands all ancestor folders and scrolls the target file into view.
</Callout>

<h2>Context menu actions</h2>
<p>Right-click any file to access:</p>
<div class="feature-grid two-col">
  <div class="feature-card">
    <div class="fc-title">Git Blame</div>
    <div class="fc-desc">Opens the <strong>Git Blame</strong> modal for the selected file — see below for details.</div>
  </div>
  <div class="feature-card">
    <div class="fc-title">Filter Graph by File</div>
    <div class="fc-desc">Filters the commit graph to show only commits that touched this file. A pill in the graph toolbar shows the active filter; click <strong>×</strong> to clear it. Also reachable from the Command Palette via <code>Show Commits Touching File</code> (aliases <code>file-history</code> / <code>log-file</code> / <code>history</code>) — that route lists every project file and doesn't open the Files sidebar.</div>
  </div>
  <div class="feature-card">
    <div class="fc-title">Open in Markdown Editor</div>
    <div class="fc-desc">Visible on <code>.md</code> / <code>.markdown</code> rows. Opens the file in an Obsidian-style live-preview editor: headings, bold/italic, links, code blocks and blockquotes render inline as you type. The eye button toggles read-only mode; <kbd>Ctrl</kbd>+<kbd>S</kbd> saves to disk.</div>
  </div>
</div>

<h2>Markdown Editor</h2>
<p>
  The Markdown Editor opens any <code>.md</code> / <code>.markdown</code> file from the repository
  with a live preview rendered in the same pane — there is no separate preview window. The
  reveal is <strong>per inline component</strong>: putting the cursor on a <code>**bold**</code>
  word reveals its <code>**</code> markers without disturbing a sibling <code>*italic*</code> on
  the same line. Block-level markers (heading <code>#</code>, blockquote <code>&gt;</code>, code
  fences) reveal on the whole line they belong to.
</p>

<h3>What renders inline</h3>
<ul>
  <li><strong>Headings</strong> — <code>#</code> through <code>######</code>, sized down progressively, with a subtle bottom border on H1/H2</li>
  <li><strong>Bold / italic / strikethrough</strong> — the asterisks, underscores and tildes are hidden when off-line</li>
  <li><strong>Inline code</strong> — monospace pill with subtle background</li>
  <li><strong>Fenced &amp; indented code blocks</strong> — full-line background, monospace, syntax-highlighted via Prism (same grammar set as DiffViewer / blame: JS/TS, Rust, Python, Go, Java, Kotlin, C/C++/C#, Swift, Lua, PowerShell, Bash, JSON/YAML/TOML, CSS/SCSS, HTML/XML, SQL, Docker, Svelte, XSD)</li>
  <li><strong>Blockquotes</strong> — left accent border, italicised muted text</li>
  <li><strong>Lists</strong> — bullet and ordered, with the markers highlighted in the accent colour</li>
  <li><strong>Task lists</strong> — <code>[ ]</code> / <code>[x]</code> checkbox markers</li>
  <li><strong>Links</strong> — the visible label shows underlined in the accent colour; the URL is dimmed when off-line</li>
  <li><strong>Horizontal rules</strong> — rendered as a thin separator line</li>
</ul>

<h3>Interactions</h3>
<table class="shortcuts-table">
  <thead><tr><th>Action</th><th>How</th></tr></thead>
  <tbody>
    <tr><td>Save</td><td><kbd>Ctrl</kbd>+<kbd>S</kbd> (or the <em>Save</em> button in the footer)</td></tr>
    <tr><td>Switch to read-only mode</td><td>Click the <strong>eye</strong> button in the modal header</td></tr>
    <tr><td>Close</td><td>Header <strong>✕</strong>, <kbd>Esc</kbd>, or backdrop click — unsaved changes trigger a confirmation</td></tr>
  </tbody>
</table>

<Callout variant="info" title="Under the hood">
  The editor is built on CodeMirror 6 with the Lezer Markdown parser. The live-preview layer is a
  ViewPlugin that walks the syntax tree of the visible viewport, applies styling decorations and
  conceals markup characters based on whether the current selection sits inside each inline
  component's range. Fenced code blocks are tokenised by Prism (via <code>Prism.tokenize</code>)
  and mapped back to CodeMirror mark decorations carrying the same <code>.token.*</code> classes
  used elsewhere in the app, so theming flows from the central CSS variables. The plugin rebuilds
  only when the document, viewport or selection changes — so it stays cheap on long files.
</Callout>

<h2>Git Blame</h2>
<p>
  The Git Blame modal shows the full content of a file annotated line-by-line with the commit that last
  modified each line. It can be opened either from the <strong>right-click context menu</strong> in the
  Files, or from the Command Palette via the <code>Blame File</code> verb (aliases <code>blame</code> /
  <code>annotate</code>) — the palette route lists every tracked file in the project, so you don't need
  the Files sidebar to be open.
</p>

<h3>Reading the blame view</h3>
<ul>
  <li><strong>Colored left border</strong> — each distinct commit gets a consistent color from a 10-color palette, making it easy to spot which lines belong to the same change</li>
  <li><strong>SHA chip</strong> — 7-character short OID of the responsible commit, shown only on the <em>first line of each group</em> (is_group_start)</li>
  <li><strong>Author &amp; date</strong> — author display name and relative date, also shown only on group-start lines</li>
  <li><strong>Commit summary</strong> — one-line message in muted text below the author row</li>
  <li><strong>Syntax highlighting</strong> — the code column is highlighted with Prism using the file's extension</li>
</ul>

<h3>Interactions</h3>
<table class="shortcuts-table">
  <thead><tr><th>Action</th><th>How</th></tr></thead>
  <tbody>
    <tr><td>Highlight all lines from the same commit</td><td>Hover any line — all lines sharing the same OID are highlighted</td></tr>
    <tr><td>Navigate to commit in graph</td><td>Click the SHA chip — the graph scrolls to that commit and the modal closes</td></tr>
    <tr><td>Close modal</td><td><kbd>Escape</kbd> or click the backdrop</td></tr>
  </tbody>
</table>

<Callout variant="info" title="Under the hood">
  Blame is computed by the Rust backend via <code>git2::Repository::blame_file()</code>
  and returned as a flat array of <code>BlameLine</code> structs (one per source line).
  Each <code>BlameLine</code> carries: <code>line_no</code>, <code>content</code>, <code>commit_oid</code>,
  <code>short_oid</code>, <code>author_name</code>, <code>author_email</code>, <code>timestamp</code>,
  <code>summary</code>, and a <code>is_group_start</code> flag set when the commit OID changes from the previous line.
</Callout>
