<script lang="ts">
  import { contributionStore } from '$lib/stores/contribution.svelte';
  import { pluginStore }       from '$lib/stores/plugin.svelte';
  import Kbd                   from '$lib/components/shared/ui/Kbd.svelte';
  import type { Keybinding }   from '$lib/utils/keybindings';

  type PluginKb = {
    plugin_name: string;
    binding:     Keybinding;
    action:      string;
    description: string;
  };

  const pluginKeybindings = $derived(
    contributionStore.forPoint('arbor:keybinding')
      .filter(c => !pluginStore.disabledPlugins.has(c.plugin_name))
      .map((c): PluginKb => {
        const p = c.payload as { key?: string; ctrl?: boolean; shift?: boolean; alt?: boolean; action?: string; description?: string };
        return {
          plugin_name: c.plugin_name,
          binding:     {
            key:         p.key   ?? '',
            ctrl:        !!p.ctrl,
            shift:       !!p.shift,
            alt:         !!p.alt,
            description: p.description ?? '',
            group:       'Plugins',
          },
          action:      p.action      ?? '',
          description: p.description ?? '',
        };
      })
      .sort((a, b) =>
        a.plugin_name.localeCompare(b.plugin_name) ||
        (a.description || a.action).localeCompare(b.description || b.action)
      )
  );
</script>

<h1>Keyboard Shortcuts</h1>

<p class="doc-lead">Arbor is designed to be fully keyboard-navigable. Most actions have a default shortcut, and every built-in binding is rebindable from Settings → Keybindings.</p>

<h2>Global shortcuts</h2>
<table class="shortcuts-table">
  <thead><tr><th>Shortcut</th><th>Action</th></tr></thead>
  <tbody>
    <tr><td><Kbd action="open_repo" size="sm" /></td><td>Open repository</td></tr>
    <tr><td><Kbd action="clone_repo" size="sm" /></td><td>Clone repository</td></tr>
    <tr><td><Kbd action="init_repo" size="sm" /></td><td>Initialize repository in folder</td></tr>
    <tr><td><Kbd action="open_recent" size="sm" /></td><td>Recent repos quick-switch</td></tr>
    <tr><td><Kbd action="repo_browser" size="sm" /></td><td>Browse remote repositories (GitHub / GitLab)</td></tr>
    <tr><td><Kbd action="command_palette" size="sm" /></td><td>Open Command Palette</td></tr>
    <tr><td><Kbd action="open_project" size="sm" /></td><td>Open project in active workspace</td></tr>
    <tr><td><Kbd action="open_from_workspace" size="sm" /></td><td>Open project from another workspace (cross-WS tab)</td></tr>
    <tr><td><Kbd action="workspace_manager" size="sm" /></td><td>Open Workspace Manager</td></tr>
    <tr><td><Kbd action="settings" size="sm" /></td><td>Open Settings</td></tr>
    <tr><td><Kbd action="toggle_docs" size="sm" /></td><td>Toggle Documentation</td></tr>
    <tr><td><kbd>Escape</kbd></td><td>Close current panel / search / modal</td></tr>
  </tbody>
</table>

<h2>Tabs &amp; navigation</h2>
<table class="shortcuts-table">
  <thead><tr><th>Shortcut</th><th>Action</th></tr></thead>
  <tbody>
    <tr><td><Kbd action="next_tab" size="sm" /></td><td>Next tab</td></tr>
    <tr><td><Kbd action="prev_tab" size="sm" /></td><td>Previous tab</td></tr>
    <tr><td><Kbd action="close_tab" size="sm" /></td><td>Close active tab</td></tr>
    <tr><td><Kbd action="jump_to_head" size="sm" /></td><td>Jump to HEAD commit in graph</td></tr>
    <tr><td><Kbd action="search" size="sm" /></td><td>Search commits (message / author / SHA)</td></tr>
  </tbody>
</table>

<h2>Panels</h2>
<p class="hint">
  <strong><Kbd action="toggle_sidebar" size="sm" /></strong> /
  <strong><Kbd action="toggle_right_sidebar" size="sm" /></strong> /
  <strong><Kbd action="toggle_bottom_panel" size="sm" /></strong> are <em>generic</em> visibility
  toggles — they collapse whatever section is open or restore the last-used one. The
  numbered Alt+Shift shortcuts below pick a specific section directly (IntelliJ-style).
</p>
<table class="shortcuts-table">
  <thead><tr><th>Shortcut</th><th>Action</th></tr></thead>
  <tbody>
    <tr><td><Kbd action="toggle_sidebar" size="sm" /></td><td>Toggle left sidebar visibility</td></tr>
    <tr><td><Kbd action="toggle_right_sidebar" size="sm" /></td><td>Toggle right sidebar visibility</td></tr>
    <tr><td><Kbd action="toggle_bottom_panel" size="sm" /></td><td>Toggle bottom panel visibility</td></tr>
    <tr><td><Kbd action="stage_view" size="sm" /></td><td>Toggle Stage area</td></tr>
    <tr><td><Kbd action="toggle_terminal" size="sm" /></td><td>Toggle terminal panel</td></tr>
    <tr><td><Kbd action="new_terminal" size="sm" /></td><td>Open new terminal tab</td></tr>
    <tr><td><Kbd action="plugin_logs" size="sm" /></td><td>Toggle Plugin Logs console</td></tr>
    <tr><td><Kbd action="toggle_keystrokes" size="sm" /></td><td>Toggle the <em>Keyboard Inputs</em> overlay (demos, screencasts) — works even inside modals</td></tr>
  </tbody>
</table>

<h2>Sidebar Sections</h2>
<p class="hint">
  IntelliJ-style numbered tool-window shortcuts. Each shortcut is silently a no-op when the
  matching ActivityBar button has been hidden via <strong>Settings → Customize Activity Bar</strong>.
</p>
<table class="shortcuts-table">
  <thead><tr><th>Shortcut</th><th>Action</th></tr></thead>
  <tbody>
    <tr><td><Kbd action="toggle_branches_sidebar" size="sm" /></td><td>Toggle Branches &amp; Stashes</td></tr>
    <tr><td><Kbd action="toggle_files_sidebar"    size="sm" /></td><td>Toggle File Tree</td></tr>
    <tr><td><Kbd action="toggle_gitflow_sidebar"  size="sm" /></td><td>Toggle Git Flow</td></tr>
    <tr><td><Kbd action="toggle_issues_sidebar"   size="sm" /></td><td>Toggle Issues (Linear / Jira)</td></tr>
    <tr><td><Kbd action="toggle_pipelines_panel"  size="sm" /></td><td>Toggle Pipelines panel</td></tr>
    <tr><td><Kbd action="toggle_reflog_sidebar"   size="sm" /></td><td>Toggle Reflog</td></tr>
    <tr><td><Kbd action="toggle_stats_sidebar"    size="sm" /></td><td>Toggle Repository Statistics</td></tr>
    <tr><td><Kbd action="toggle_security_sidebar" size="sm" /></td><td>Toggle Security / Vulnerability Dashboard</td></tr>
    <tr><td><Kbd action="toggle_mr_sidebar"       size="sm" /></td><td>Toggle Pull / Merge Requests</td></tr>
  </tbody>
</table>

<h2>Git</h2>
<table class="shortcuts-table">
  <thead><tr><th>Shortcut</th><th>Action</th></tr></thead>
  <tbody>
    <tr><td><Kbd action="fetch" size="sm" /></td><td>Fetch all remotes</td></tr>
    <tr><td><Kbd action="refresh_graph" size="sm" /></td><td>Refresh graph (same as the fetch button in the status bar)</td></tr>
    <tr><td><Kbd action="pull" size="sm" /></td><td>Pull current branch</td></tr>
    <tr><td><Kbd action="push" size="sm" /></td><td>Push current branch</td></tr>
    <tr><td><Kbd action="new_branch" size="sm" /></td><td>Create new branch</td></tr>
    <tr><td><Kbd action="stash" size="sm" /></td><td>Stash changes</td></tr>
    <tr><td><Kbd action="stage_all" size="sm" /></td><td>Stage all changes</td></tr>
    <tr><td><Kbd action="unstage_all" size="sm" /></td><td>Unstage all changes</td></tr>
  </tbody>
</table>

<h2>Stage area</h2>
<table class="shortcuts-table">
  <thead><tr><th>Shortcut</th><th>Action</th></tr></thead>
  <tbody>
    <tr><td><Kbd action="commit" size="sm" /></td><td>Commit (when focus is in message field)</td></tr>
    <tr><td><Kbd action="commit_and_push" size="sm" /></td><td>Commit and push current branch in one go</td></tr>
  </tbody>
</table>

<h2>Diff viewer</h2>
<table class="shortcuts-table">
  <thead><tr><th>Shortcut</th><th>Action</th></tr></thead>
  <tbody>
    <tr><td><Kbd action="next_chunk" size="sm" /></td><td>Jump to next change chunk</td></tr>
    <tr><td><Kbd action="prev_chunk" size="sm" /></td><td>Jump to previous change chunk</td></tr>
    <tr><td><Kbd action="diff_split" size="sm" /></td><td>Split view</td></tr>
    <tr><td><Kbd action="diff_unified" size="sm" /></td><td>Unified view</td></tr>
  </tbody>
</table>

<h2>File / Folder picker</h2>
<p class="hint">
  Shortcuts available inside the file/folder picker dialog (Open, Clone destination,
  Save As, plugin file fields, etc.). Most are dialog-scoped — they only fire while
  the picker is open.
</p>
<table class="shortcuts-table">
  <thead><tr><th>Shortcut</th><th>Action</th></tr></thead>
  <tbody>
    <tr><td><kbd>Ctrl</kbd>+<kbd>L</kbd></td><td>Edit the path directly (address bar) — type with ghost-text autocompletion</td></tr>
    <tr><td><kbd>Tab</kbd> in address bar</td><td>Accept the ghost-text autocomplete suggestion</td></tr>
    <tr><td><kbd>Ctrl</kbd>+<kbd>N</kbd></td><td>Create a new file in the current folder</td></tr>
    <tr><td><kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>N</kbd></td><td>Create a new folder in the current folder</td></tr>
    <tr><td><Kbd action="toggle_sidebar" size="sm" /></td><td>Collapse / expand the picker sidebar (same global shortcut)</td></tr>
    <tr><td><kbd>Alt</kbd>+<kbd>←</kbd> / <kbd>Alt</kbd>+<kbd>→</kbd></td><td>Back / Forward through navigation history</td></tr>
    <tr><td><kbd>Backspace</kbd></td><td>Go up one folder</td></tr>
    <tr><td><kbd>↑</kbd> / <kbd>↓</kbd></td><td>Move selection in the file list</td></tr>
    <tr><td><kbd>F2</kbd></td><td>Rename the selected entry</td></tr>
    <tr><td><kbd>Delete</kbd></td><td>Delete the selected entry (asks for confirmation)</td></tr>
    <tr><td><kbd>Enter</kbd></td><td>Open folder · open file · confirm pick · confirm delete</td></tr>
    <tr><td>Type any letter</td><td>Type-ahead — keystrokes route into the filter field automatically</td></tr>
    <tr><td><kbd>↓</kbd> in filter field</td><td>Jump focus to the first matching entry</td></tr>
  </tbody>
</table>

<h2>Context menus</h2>
<table class="shortcuts-table">
  <thead><tr><th>Target</th><th>How to open</th></tr></thead>
  <tbody>
    <tr><td>Commit (graph)</td><td>Right-click commit row</td></tr>
    <tr><td>Branch (sidebar)</td><td>Right-click branch item</td></tr>
    <tr><td>File (stage area / diff list)</td><td>Right-click file entry</td></tr>
    <tr><td>Tab (tab bar)</td><td>Right-click tab</td></tr>
  </tbody>
</table>

<h2>Where shortcuts surface in the UI</h2>
<p>Built-in shortcuts are rendered live next to the action wherever it appears:</p>
<ul>
  <li><strong>Main menu</strong> (hamburger top-left) — IntelliJ-style right-aligned hint on each row.</li>
  <li><strong>Command Palette</strong> (<kbd>Ctrl+K</kbd>) — small <kbd>kbd</kbd> badge at the right of the row.</li>
  <li><strong>Right-click context menus</strong> — branch, commit, tab, stage entries.</li>
  <li><strong>Tooltips</strong> on Activity Bar, Status Bar and TitleBar buttons (e.g. hovering the Fetch button shows <em>Fetch from remote (Ctrl+Shift+F)</em>).</li>
</ul>
<p>All bindings flow from a single source of truth, so a remap in <strong>Settings → Keybindings</strong> updates every hint in place — no restart required.</p>

<h2>Customizing shortcuts</h2>
<p>All built-in shortcuts are rebindable via <strong>Settings → Keybindings</strong>. Click any shortcut chip to record a new key combination; press <kbd>Escape</kbd> while recording to cancel. A reset icon appears next to modified bindings.</p>

<h2>Plugin shortcuts</h2>
<p>Plugins can register their own keybindings using <code>arbor.keybinding.register()</code>. Plugin shortcuts also appear in a read-only <strong>Plugins</strong> section at the bottom of Settings → Keybindings. They fire the associated Lua action directly and take priority if no built-in binding is mapped to the same combination.</p>

{#if pluginKeybindings.length > 0}
  <p>Currently registered by enabled plugins:</p>
  <table class="shortcuts-table">
    <thead><tr><th>Shortcut</th><th>Action</th><th>Plugin</th></tr></thead>
    <tbody>
      {#each pluginKeybindings as kb (kb.plugin_name + ':' + kb.action)}
        <tr>
          <td><Kbd binding={kb.binding} size="sm" /></td>
          <td>{kb.description || kb.action}</td>
          <td><code>{kb.plugin_name}</code></td>
        </tr>
      {/each}
    </tbody>
  </table>
{:else}
  <p class="hint">No plugin keybindings registered. Enable a plugin that calls <code>arbor.keybinding.register()</code> to populate this list.</p>
{/if}
