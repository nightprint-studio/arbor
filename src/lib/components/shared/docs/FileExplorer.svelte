<script lang="ts">
  import Callout from '$lib/components/shared/ui/Callout.svelte';
  import Kbd     from '$lib/components/shared/internal/Kbd.svelte';
</script>

<h1>File Explorer</h1>

<p class="doc-lead">The built-in <strong>File Explorer</strong> browses your real filesystem from inside Arbor — drives, folders and files, with git awareness, previews and full keyboard navigation. The same explorer also powers every file/folder picker in the app, so choosing a path looks and behaves exactly like browsing.</p>

<Callout variant="info" title="Two ways it shows up">
  As a <strong>browser</strong> it opens in its own dedicated window (or as a modal). As a <strong>picker</strong> it opens focused, in-app, with a Cancel / Confirm footer — whenever an action needs you to choose a file, a folder, or a save location.
</Callout>

<h2>Opening the explorer</h2>
<table class="shortcuts-table">
  <thead><tr><th>How</th><th>Result</th></tr></thead>
  <tbody>
    <tr><td>Command Palette → <strong>Open File Explorer</strong></td><td>Dedicated, frameless explorer window</td></tr>
    <tr><td>System-tray menu → <strong>Open File Explorer</strong></td><td>Same dedicated window (works while Arbor is minimized to tray)</td></tr>
    <tr><td>Global shortcut <Kbd label="Ctrl+Shift+E" /></td><td>Opens / focuses the window even when Arbor isn't focused — <em>opt-in</em>, enable it in Settings → File Explorer</td></tr>
  </tbody>
</table>
<p>By default, re-summoning <strong>focuses the existing window</strong> instead of opening a second one. Turn on <strong>Always open a new window</strong> (in the explorer's settings) if you prefer a fresh window each time.</p>

<h2>Layout</h2>
<ul class="step-list">
  <li><strong>Sidebar</strong> — Library (Overview / Settings), Recents, Favourites, Devices, and Projects (your Arbor-registered repos, grouped by workspace). Sections can be <strong>reordered and hidden</strong> from the settings page, or right-click a section header to hide it. Toggle the whole sidebar with <Kbd label="Ctrl+B" />.</li>
  <li><strong>Tabs</strong> — open several locations at once; each tab keeps its own history.</li>
  <li><strong>Address bar</strong> — click it (or <Kbd label="Ctrl+L" />) to type a path, with <strong>ghost-text autocomplete</strong> (press <Kbd label="Tab" /> to complete). The breadcrumb is clickable.</li>
  <li><strong>Views</strong> — Details list or Medium / Large / Extra-large icon grids; the choice is remembered per folder. Large grids show image thumbnails.</li>
  <li><strong>Filter & search</strong> — type to filter the current folder (wildcards <code>*</code> <code>?</code> supported); toggle <strong>recursive search</strong> to match inside every subfolder, and <strong>show hidden</strong> to include dotfiles.</li>
</ul>

<h2>Navigation &amp; keyboard</h2>
<table class="shortcuts-table">
  <thead><tr><th>Action</th><th>Keys</th></tr></thead>
  <tbody>
    <tr><td>Back / Forward</td><td><Kbd label="Alt+Left" /> / <Kbd label="Alt+Right" /></td></tr>
    <tr><td>Up one folder</td><td><Kbd label="Backspace" /></td></tr>
    <tr><td>Move selection</td><td><Kbd label="Up" /> / <Kbd label="Down" /> (and <Kbd label="Left" /> / <Kbd label="Right" /> in icon grids)</td></tr>
    <tr><td>Open folder / file</td><td><Kbd label="Enter" /> or double-click</td></tr>
    <tr><td>Edit the address</td><td><Kbd label="Ctrl+L" /></td></tr>
    <tr><td>Type-ahead filter</td><td>Just start typing</td></tr>
    <tr><td>Select all</td><td><Kbd label="Ctrl+A" /></td></tr>
    <tr><td>Open the explorer settings</td><td><Kbd label="Ctrl+," /></td></tr>
  </tbody>
</table>

<h2>Managing files</h2>
<p>Right-click an item (or the background) for the full menu; common actions are also on the keyboard:</p>
<ul class="step-list">
  <li><strong>New folder / file</strong>, <strong>Rename</strong> (<Kbd label="F2" />), <strong>Cut / Copy / Paste</strong> (<Kbd label="Ctrl+X" /> / <Kbd label="Ctrl+C" /> / <Kbd label="Ctrl+V" />).</li>
  <li><strong>Delete</strong> to Recycle Bin (<Kbd label="Delete" />) or permanently (<Kbd label="Shift+Delete" />).</li>
  <li><strong>Move</strong> by dragging items onto a folder.</li>
  <li><strong>Compress to ZIP</strong> / <strong>Extract here</strong>, <strong>Set as desktop background</strong> (images), <strong>Open with default app</strong>, <strong>Reveal in File Explorer</strong>, <strong>Copy Path</strong>, and <strong>Properties</strong>.</li>
</ul>

<h2>Right rail: Preview, Info, Changes</h2>
<p>The right rail renders a live <strong>Preview</strong> of the selected file (image, video, audio, or syntax-highlighted text), an <strong>Info</strong> panel with size / dates / path (and a repository section for repo folders), and a <strong>Changes</strong> panel when inside a git repo. Resize or expand the rail; toggle the preview with <Kbd label="Ctrl+Shift+B" />.</p>

<h2>Git awareness</h2>
<p>Inside a git repository the explorer overlays each row with a status badge — <strong>modified</strong>, <strong>staged</strong>, <strong>untracked</strong>, <strong>deleted</strong>, <strong>renamed</strong>, <strong>conflicted</strong> (ignored items are dimmed) — and folders roll up to their strongest descendant state. The footer shows the current branch with ahead / behind counts.</p>
<ul class="step-list">
  <li><strong>Stage</strong>, <strong>Unstage</strong>, <strong>Discard changes</strong> (with confirmation), and <strong>Add to .gitignore</strong> from the right-click menu.</li>
  <li><strong>Switch branch</strong> — click the footer branch chip (or right-click a repo) for a filterable, keyboard-first branch picker. Uses a safe checkout that refuses to overwrite uncommitted changes.</li>
  <li>A folder that is itself a repository is flagged when browsing its parent (branch chip / corner badge), with coloured workspace dots when it's registered in Arbor.</li>
  <li><strong>Open in Arbor</strong> brings the main window forward and opens the repo, so the heavy operations (diff, log, blame, commit) happen in Arbor's full git UI.</li>
</ul>
<Callout variant="tip" title="Off by default">
  Git awareness is behind a master switch and starts <strong>off</strong>, so plain browsing issues no git checks and stays fast. Turn it on in the explorer's settings or in Settings → File Explorer.
</Callout>

<h2>Using it as a picker</h2>
<p>When an action asks you to choose a path — opening or cloning a repository, importing or exporting themes and workspaces, picking an executable, exporting the graph / docs / statistics, saving a Studio document, plugin file pickers, and more — the explorer opens <strong>focused in-app</strong>: a single browse view with the sidebar, breadcrumb, search, and git overlays, plus a Cancel / Confirm footer. It never opens a separate window.</p>
<table class="shortcuts-table">
  <thead><tr><th>Mode</th><th>What you pick</th></tr></thead>
  <tbody>
    <tr><td><strong>Folder</strong></td><td>The folder you're in, or a selected sub-folder.</td></tr>
    <tr><td><strong>File</strong></td><td>A file (optionally limited to certain extensions — non-matching files are hidden). Some pickers allow selecting multiple files with <Kbd label="Ctrl" /> / <Kbd label="Shift" />-click.</td></tr>
    <tr><td><strong>Save</strong></td><td>A folder plus a filename typed in the footer; a warning appears if a file with that name already exists.</td></tr>
  </tbody>
</table>
<p>Confirm with the footer button, by double-clicking a file (file mode), or with <Kbd label="Ctrl+Enter" /> from anywhere; <Kbd label="Esc" /> cancels.</p>

<h2>Links in the address bar</h2>
<p>Besides filesystem paths, the address bar understands links:</p>
<ul class="step-list">
  <li><strong>Arbor deep links</strong> (<code>arbor://…</code>) — paste one (open repository, jump to a commit, checkout a branch, open an MR / pipeline) and it's routed to Arbor's deep-link handler, which brings the main window forward. Because you typed it yourself, manual links work even when the deep-link feature is otherwise off — no need to enable it first (the per-action confirmation still applies). <code>arbor://settings</code> opens the explorer's own settings page.</li>
  <li><strong>External links</strong> — custom schemes like <code>vscode://</code>, <code>mailto:</code> or <code>slack://</code> can open in their associated app. This is <strong>off by default</strong>; enable it under <em>Open external links</em> in the explorer settings. Each link prompts for confirmation (Chrome-style) with an <em>Always allow this scheme</em> option to skip future prompts.</li>
  <li><strong>Web links</strong> (<code>http</code> / <code>https</code>) open in your default browser — a separate opt-in (<em>Open web links</em>) on top of external links, also off by default.</li>
</ul>

<h2>Settings</h2>
<p>Open the explorer's own settings page by typing <code>arbor://settings</code> in the address bar, the sidebar <strong>Settings</strong> item, or <Kbd label="Ctrl+," />. The same switches live under <strong>Settings → File Explorer</strong>. You can tune git awareness, the global shortcut (rebindable) and always-new-window behaviour, the default view / sort / on-open folder, show-hidden and recursive search, the maximum number of recent folders, and whether the address bar may open external / web links — plus <strong>Reset</strong> actions for the per-folder view memory, recent folders, sidebar / panel layout, and remembered link schemes.</p>
