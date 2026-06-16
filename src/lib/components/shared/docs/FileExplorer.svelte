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
<p>By default, re-summoning <strong>focuses the existing window</strong> instead of opening a second one. Turn on <strong>Always open a new window</strong> (in the explorer's settings) if you prefer a fresh window each time. Closing a window (its close button or <Kbd label="Ctrl+W" /> on the last tab) closes it for good — the next open starts fresh.</p>
<p>Open windows work <strong>together</strong>: the clipboard is shared, so you can copy in one and paste in another, and you can <strong>drag items from one explorer window onto another</strong> to move them into the folder it's showing — a ghost follows the cursor across the desktop, and the target window comes forward on drop.</p>

<h2>Layout</h2>
<ul class="step-list">
  <li><strong>Sidebar</strong> — Library (Overview / Recycle Bin / Settings), Recents, Favourites, Saved searches, Devices, <strong>Linux</strong> (your installed WSL distributions, browsable via <code>\\wsl.localhost\</code> — Windows only), and Projects (your Arbor-registered repos, grouped by workspace). Sections can be <strong>reordered and hidden</strong> from the settings page, or right-click a section header to hide it. <strong>Favourites</strong> are pinnable: right-click any folder → <em>Add to Favourites</em> (remove with the × on the pinned row, or right-click it). It's fully arrow-navigable: reach it with <Kbd label="F6" />, which cycles focus across the explorer's panes — sidebar → list → right panel → right activity bar (<Kbd label="Shift+F6" /> goes back), no Tab needed. Within the sidebar, <Kbd label="Up" /> / <Kbd label="Down" /> move between headers and items, <Kbd label="Right" /> / <Kbd label="Left" /> expand / collapse a section or workspace group, <Kbd label="Enter" /> opens. Toggle the whole sidebar with <Kbd label="Ctrl+B" />.</li>
  <li><strong>Tabs</strong> — open several locations at once; each tab keeps its own history.</li>
  <li><strong>Address bar</strong> — click it (or <Kbd label="Ctrl+L" />) to type a path, with <strong>ghost-text autocomplete</strong> (press <Kbd label="Tab" /> to complete). The breadcrumb is clickable. Shell-style shortcuts are expanded on <Kbd label="Enter" />: <code>%appdata%</code>, <code>$HOME</code>, <code>${XDG_CONFIG_HOME}</code> and a leading <code>~</code>. The virtual names <code>%appdata%</code> / <code>%localappdata%</code> / <code>%home%</code> resolve to the right OS folder on every platform.</li>
  <li><strong>Views</strong> — Details list or Medium / Large / Extra-large icon grids; the choice is remembered per folder. Large grids show image thumbnails.</li>
  <li><strong>Columns</strong> (details view) — click a column header to sort by it (the <strong>↑ / ↓</strong> arrow sits after the label and flips the direction on a second click). <strong>Drag a header</strong> to reorder columns, and <strong>right-click any header</strong> to show / hide columns or reset them. Available columns: Name (always shown, always first), Date modified, Type, Size, Date created, Extension and Git status — only the first four are on by default. The whole set is persisted to your Arbor config.</li>
  <li><strong>Filter & search</strong> — type to filter the current folder (wildcards <code>*</code> <code>?</code> supported); toggle <strong>recursive search</strong> to match inside every subfolder, and <strong>show hidden</strong> to include dotfiles. The <strong>Filters</strong> button adds advanced filters (by kind, size and modified date); the <strong>bookmark</strong> button saves the current query + filters + folder as a <strong>Saved search</strong> in the sidebar — click it to re-run, right-click (or the ×) to remove.</li>
  <li><strong>Overview</strong> — the landing dashboard shows real storage stats: total capacity, free and used space, and a per-drive usage bar (Windows, macOS and Linux), plus your devices and quick-access locations.</li>
  <li><strong>Recycle Bin</strong> — a dedicated Library view lists trashed items (name, original location, deletion date) with <strong>Restore</strong>, permanent <strong>Delete</strong>, and <strong>Empty</strong>. On Windows and Linux, Restore puts items back to their original location; on macOS (which records no original path) it recovers them to the <strong>Desktop</strong>.</li>
</ul>

<h2>Navigation &amp; keyboard</h2>
<p>The explorer is fully keyboard-driven — no mouse required. It opens with a cursor already on the first item (and focus on the list, or the filename field in a save dialog), so you can start arrowing around or typing immediately. As a picker, the whole flow is "type to filter → <Kbd label="Down" /> → <Kbd label="Enter" /> / <Kbd label="Ctrl+Enter" />".</p>
<table class="shortcuts-table">
  <thead><tr><th>Action</th><th>Keys</th></tr></thead>
  <tbody>
    <tr><td>Back / Forward</td><td><Kbd label="Alt+Left" /> / <Kbd label="Alt+Right" /></td></tr>
    <tr><td>Up one folder</td><td><Kbd label="Backspace" /></td></tr>
    <tr><td>Move the cursor</td><td><Kbd label="Up" /> / <Kbd label="Down" /> (and <Kbd label="Left" /> / <Kbd label="Right" /> in icon grids)</td></tr>
    <tr><td>First / last item</td><td><Kbd label="Home" /> / <Kbd label="End" /></td></tr>
    <tr><td>Jump a page</td><td><Kbd label="PageUp" /> / <Kbd label="PageDown" /></td></tr>
    <tr><td>Extend the selection</td><td>Hold <Kbd label="Shift" /> while moving the cursor</td></tr>
    <tr><td>Open folder / file</td><td><Kbd label="Enter" /> or double-click</td></tr>
    <tr><td>Open the context menu</td><td><Kbd action="open_context_menu" /></td></tr>
    <tr><td>Properties (Info panel)</td><td><Kbd label="Alt+Enter" /></td></tr>
    <tr><td>Cycle panes (sidebar · list · panel · activity bar)</td><td><Kbd label="F6" /> / <Kbd label="Shift+F6" /></td></tr>
    <tr><td>Edit the address</td><td><Kbd label="Ctrl+L" /></td></tr>
    <tr><td>Type-ahead filter</td><td>Just start typing — then <Kbd label="Down" /> steps into the list (or <Kbd label="Enter" /> opens the first match)</td></tr>
    <tr><td>Select all</td><td><Kbd label="Ctrl+A" /></td></tr>
    <tr><td>Clear the selection (in a picker, target the current folder itself)</td><td><Kbd label="Esc" /> or click empty space</td></tr>
    <tr><td>Undo / Redo the last file operation</td><td><Kbd label="Ctrl+Z" /> / <Kbd label="Ctrl+Shift+Z" /></td></tr>
    <tr><td>Close the tab (or the explorer, if it's the last)</td><td><Kbd label="Ctrl+W" /></td></tr>
    <tr><td>Open the explorer settings</td><td><Kbd label="Ctrl+," /></td></tr>
  </tbody>
</table>

<h2>Managing files</h2>
<p>Right-click an item (or the background) for the full menu — or open it from the keyboard on the cursor row with <Kbd action="open_context_menu" />. The menu is fully keyboard-driven: <Kbd label="Up" /> / <Kbd label="Down" /> move, type a letter to jump, <Kbd label="Right" /> / <Kbd label="Left" /> open / close a submenu (e.g. <strong>Git</strong>), <Kbd label="Enter" /> activates, <Kbd label="Esc" /> closes. Common actions are also on the keyboard directly:</p>
<ul class="step-list">
  <li><strong>New folder / file</strong>, <strong>Rename</strong> (<Kbd label="F2" />), <strong>Cut / Copy / Paste</strong> (<Kbd label="Ctrl+X" /> / <Kbd label="Ctrl+C" /> / <Kbd label="Ctrl+V" />).</li>
  <li><strong>Delete</strong> to Recycle Bin (<Kbd label="Delete" />) or permanently (<Kbd label="Shift+Delete" />) — both ask for confirmation first.</li>
  <li><strong>Move</strong> by dragging items onto a folder.</li>
  <li><strong>Undo / Redo</strong> (<Kbd label="Ctrl+Z" /> / <Kbd label="Ctrl+Shift+Z" />, or the toolbar arrows) covers create, rename, move, paste and Recycle-Bin delete — an undone delete is restored from the Recycle Bin.</li>
  <li><strong>Open in editor</strong> — open the item (or the current folder) in your IDE; a submenu lists the detected and custom editors with the configured default badged. <strong>Open in Terminal</strong> opens the OS terminal rooted at the folder (a file uses its parent).</li>
  <li><strong>Duplicate</strong> copies the selection in place (<code>report (2).pdf</code>). <strong>Rename</strong> on a single item edits inline; on several it opens a <strong>batch rename</strong> dialog (find → replace, case, sequence numbering, with a live preview). Large copies / moves / duplicates show a <strong>progress bar with a Cancel button</strong>; the footer shows the <strong>count and total size</strong> of the current selection, and a folder's Info panel can <strong>calculate its size</strong> recursively.</li>
  <li><strong>Compress to ZIP</strong> / <strong>Extract here</strong>, <strong>Set as desktop background</strong> (images), <strong>Open with default app</strong>, <strong>Reveal in File Explorer</strong>, <strong>Copy Path</strong>, and <strong>Properties</strong>.</li>
</ul>
<p>Pasting an item whose name already exists in the destination asks what to do: <strong>Replace</strong> (merge folders and overwrite matching files), <strong>Keep both</strong> (the pasted copy gets a “ (2)” suffix), or <strong>Cancel</strong>.</p>

<h2>Right rail: Preview, Info, Changes</h2>
<p>The right rail renders a live <strong>Preview</strong> of the selected file (image, video, audio, or syntax-highlighted text), an <strong>Info</strong> panel with size / dates / path (and a repository section for repo folders), and a <strong>Changes</strong> panel when inside a git repo. Resize or expand the rail; toggle the preview with <Kbd label="Ctrl+Shift+B" />. <Kbd label="Alt+Enter" /> opens the Info panel for the cursor item (Windows-style Properties); from there a button opens the OS-native Properties sheet.</p>

<h2>Git awareness</h2>
<p>Inside a git repository the explorer overlays each row with a status badge — <strong>modified</strong>, <strong>staged</strong>, <strong>untracked</strong>, <strong>deleted</strong>, <strong>renamed</strong>, <strong>conflicted</strong> (ignored items are dimmed) — and folders roll up to their strongest descendant state. The footer shows the current branch with ahead / behind counts.</p>
<p>Right-clicking an entry inside (or that is) a repository shows a single <strong>Git</strong> entry that expands into two grouped sections:</p>
<ul class="step-list">
  <li><strong>Project</strong> — actions on the whole repository: <strong>Checkout branch…</strong> (a filterable, keyboard-first picker using a safe checkout that refuses to overwrite uncommitted changes; also reachable from the footer branch chip), <strong>Open in Arbor</strong> (brings the main window forward and opens the repo there, so the heavy operations — diff, log, blame, commit — happen in Arbor's full git UI), and <strong>Copy project link</strong> (a shareable <code>arbor://</code> link to open the repo, built from its remote).</li>
  <li><strong>Element</strong> — actions scoped to the right-clicked file(s) / folder(s): <strong>Stage</strong>, <strong>Unstage</strong>, <strong>Discard changes</strong> (with confirmation), and <strong>Add to .gitignore</strong>.</li>
  <li>A folder that is itself a repository is flagged when browsing its parent (branch chip / corner badge), with coloured workspace dots when it's registered in Arbor.</li>
</ul>
<Callout variant="tip" title="Off by default">
  Git awareness is behind a master switch and starts <strong>off</strong>, so plain browsing issues no git checks and stays fast. Turn it on in the explorer's settings or in Settings → File Explorer.
</Callout>

<h2>Using it as a picker</h2>
<p>When an action asks you to choose a path — opening or cloning a repository, importing or exporting themes and workspaces, picking an executable, exporting the graph / docs / statistics, saving a Studio document, plugin file pickers, and more — the explorer opens <strong>focused in-app</strong>: a single browse view with the sidebar, breadcrumb, search, and git overlays, plus a Cancel / Confirm footer. It never opens a separate window.</p>
<table class="shortcuts-table">
  <thead><tr><th>Mode</th><th>What you pick</th></tr></thead>
  <tbody>
    <tr><td><strong>Folder</strong></td><td>The folder you're in by default, or a sub-folder once you select one. Click empty space or press <Kbd label="Esc" /> to clear the selection and target the current folder itself.</td></tr>
    <tr><td><strong>File</strong></td><td>A file (optionally limited to certain extensions — non-matching files are hidden). Some pickers allow selecting multiple files with <Kbd label="Ctrl" /> / <Kbd label="Shift" />-click.</td></tr>
    <tr><td><strong>Save</strong></td><td>A folder plus a filename typed in the footer; a warning appears if a file with that name already exists.</td></tr>
  </tbody>
</table>
<p>Confirm with the footer button, by double-clicking a file (file mode), or with <Kbd label="Ctrl+Enter" /> from anywhere; <Kbd label="Esc" /> cancels.</p>

<h2>Links in the address bar</h2>
<p>Besides filesystem paths, the address bar understands links:</p>
<ul class="step-list">
  <li><strong>Arbor deep links</strong> (<code>arbor://…</code>) — paste one (open repository, jump to a commit, checkout a branch, open an MR / pipeline) and it's routed to Arbor's deep-link handler, which brings the main window forward. Because you typed it yourself, manual links work even when the deep-link feature is otherwise off — no need to enable it first (the per-action confirmation still applies). <code>arbor://overview</code> and <code>arbor://settings</code> jump to the explorer's own Overview dashboard and Settings page — and because both are addressable, the path stays editable from them (click the address bar or press <Kbd label="Ctrl+L" />) without first navigating to a folder.</li>
  <li><strong>External links</strong> — custom schemes like <code>vscode://</code>, <code>mailto:</code> or <code>slack://</code> can open in their associated app. This is <strong>off by default</strong>; enable it under <em>Open external links</em> in the explorer settings. Each link prompts for confirmation (Chrome-style) with an <em>Always allow this scheme</em> option to skip future prompts.</li>
  <li><strong>Web links</strong> (<code>http</code> / <code>https</code>) open in your default browser — a separate opt-in (<em>Open web links</em>) on top of external links, also off by default.</li>
</ul>

<h2>Settings</h2>
<p>Open the explorer's own settings page by typing <code>arbor://settings</code> in the address bar, the sidebar <strong>Settings</strong> item, or <Kbd label="Ctrl+," />. The same switches live under <strong>Settings → File Explorer</strong>. You can tune git awareness, the global shortcut (rebindable) and always-new-window behaviour, the default view / sort / on-open folder, show-hidden and recursive search, the maximum number of recent folders, and whether the address bar may open external / web links — plus <strong>Reset</strong> actions for the per-folder view memory, recent folders, sidebar / panel layout, and remembered link schemes.</p>
<Callout variant="info" title="Be the default file explorer">
  Turn on <strong>Open in the built-in explorer</strong> (Settings → File Explorer) and the app's “Open / Reveal in File Explorer” actions — worktree info, plugin folders, notification reveals — open here instead of the OS file manager, focusing an existing tab when that folder is already open. Off by default. The explorer's own <strong>Reveal in File Explorer</strong> item always uses the OS, as an explicit way out to the system shell.
</Callout>
