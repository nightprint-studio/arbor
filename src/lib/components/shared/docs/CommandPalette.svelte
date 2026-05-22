<script lang="ts">
  import { highlight }          from '$lib/utils/diff-formatter';
  import { contributionStore }  from '$lib/stores/contribution.svelte';
  import { pluginStore }        from '$lib/stores/plugin.svelte';

  type PluginCmd = {
    plugin_name: string;
    item_id:     string;
    title:       string;
    description: string;
    icon:        string;
    group:       string;
  };

  const pluginCommands = $derived(
    contributionStore.forPoint('arbor:command-palette')
      .filter(c => !pluginStore.disabledPlugins.has(c.plugin_name))
      .map((c): PluginCmd => {
        const p = c.payload as { title?: string; description?: string; icon?: string; group?: string };
        return {
          plugin_name: c.plugin_name,
          item_id:     c.item_id,
          title:       p.title       ?? c.item_id,
          description: p.description ?? '',
          icon:        p.icon        ?? 'Zap',
          group:       p.group       ?? '',
        };
      })
      .sort((a, b) =>
        a.plugin_name.localeCompare(b.plugin_name) ||
        a.title.localeCompare(b.title)
      )
  );
</script>

<h1>Command Palette</h1>
<p class="doc-lead">The Command Palette (<kbd>Ctrl+K</kbd>) is a strictly <strong>verb-first</strong> launcher: you always pick an action first, then (when the action takes a target) refine to a specific branch / tag / commit / file. Ambiguity is removed by design — the palette always shows what will happen on <kbd>Enter</kbd>.</p>

<h2>Opening &amp; navigating</h2>
<table class="shortcuts-table">
  <thead><tr><th>Key</th><th>Action</th></tr></thead>
  <tbody>
    <tr><td><kbd>Ctrl+K</kbd></td><td>Open / close the palette</td></tr>
    <tr><td><kbd>↑</kbd> / <kbd>↓</kbd></td><td>Move selection up / down</td></tr>
    <tr><td><kbd>Enter</kbd></td><td>Pick the highlighted command (Phase 1) or run it on the highlighted target (Phase 2)</td></tr>
    <tr><td><kbd>Tab</kbd></td><td>Accept ghost-text autocompletion</td></tr>
    <tr><td><kbd>Backspace</kbd></td><td><em>On empty input, in Phase 2</em>: remove the verb chip and go back to Phase 1</td></tr>
    <tr><td><kbd>Esc</kbd></td><td>Close the palette</td></tr>
  </tbody>
</table>

<h2>Two phases: pick a command, then a target</h2>
<p>The palette is a two-step flow. In <strong>Phase 1</strong> you autocomplete a command; in <strong>Phase 2</strong> the command becomes a chip at the left of the input, and the list filters to the targets for that command.</p>

<h3>Phase 1 — Commands</h3>
<p>With an empty input the list shows every runnable command, grouped by category. Verbs (which open a target picker) always come first; leaf actions follow, grouped by area:</p>
<ul>
  <li><strong>Branch</strong> — <em>Checkout</em>, <em>Merge</em>, <em>Delete Branch</em>, <em>Rename Branch</em>, <em>Push Branch</em>, <em>Focus Branch in Graph</em></li>
  <li><strong>Navigate</strong> — <em>Go to Commit</em>, <em>Go to Tag</em>, <em>Blame File</em>, <em>Show Commits Touching File</em></li>
  <li><strong>Commit</strong> — <em>Cherry-pick</em>, <em>Revert Commit</em>, <em>Reset Soft / Mixed / Hard</em>, <em>Create Branch Here</em>, <em>Create Tag</em> (Enter on empty input tags HEAD), <em>Copy Commit SHA</em></li>
  <li><strong>Stash</strong> — <em>Apply Stash</em>, <em>Pop Stash</em>, <em>Drop Stash</em></li>
  <li><strong>Tag</strong> — <em>Delete Tag (local)</em>, <em>Delete Tag (local + origin)</em>, <em>Push Tag</em></li>
  <li><strong>Remote</strong> — <em>Fetch from Remote</em>, <em>Pull from Remote</em>, <em>Push Branch to Remote</em></li>
  <li><strong>Tabs</strong> — <em>Switch Tab</em>, <em>Close Tab</em></li>
  <li><strong>Repository</strong> — <em>Open Recent Repository</em></li>
  <li><strong>Merge Requests</strong> — <em>View MR / PR Detail</em> (opens the detail modal for a pull / merge request), <em>Open Pull / Merge Request</em> (opens the create MR/PR modal)</li>
  <li><strong>Appearance</strong> — <em>Switch Theme</em></li>
  <li><strong>Repository actions (leaves)</strong> — Open / Init / Clone / Reload Repository</li>
  <li><strong>Workspaces</strong> — <em>Switch Workspace</em>, <em>Open Project</em>, <em>Open from Workspace</em>, Manage Workspaces, Create Workspace</li>
  <li><strong>Worktrees</strong> — <em>Worktree Info</em>, <em>Switch Worktree</em></li>
  <li><strong>Deep Links</strong> — <em>Copy arbor:// Link to Commit / Checkout Branch / Branch Worktree / MR</em> (the <em>Open Repository</em> link is a leaf action under <strong>Copy</strong>)</li>
  <li><strong>Linked Worktrees</strong> — Manage Linked Worktrees, Link this Worktree…, Unlink from "&lt;link&gt;", Enable / Disable Sync for "&lt;link&gt;" (latter four shown only when applicable to the current repo)</li>
  <li><strong>Tabs (leaves)</strong> — Close Current Tab, Next / Previous Tab</li>
  <li><strong>Git (leaves)</strong> — Pull, Push, Fetch All Remotes, New Branch, Stash Changes</li>
  <li><strong>Stage &amp; Commit</strong> — Commit, Amend Last Commit, Stage All, Unstage All, Discard All, Undo Last Commit</li>
  <li><strong>Rebase / Merge</strong> — Continue / Skip / Abort Rebase, Abort Merge (visible only while the repo is in that state)</li>
  <li><strong>Panels</strong> — Toggle Stage / Detail / Terminal / Jobs / Notifications / Sidebar; Show Branches / Git Flow / MRs / Issues / Files / Reflog / Stats / Pipelines</li>
  <li><strong>Copy</strong> — Copy Current Branch Name, Copy Current SHA, Copy <code>origin</code> URL, <em>Copy arbor:// Link to Open Repository</em></li>
  <li><strong>System</strong> — Settings, Plugin Manager, Reload Plugins, Documentation, About Arbor</li>
  <li><strong>Submodules</strong> — Update All Submodules</li>
  <li><strong>Navigation</strong> — Jump to HEAD, Open in IDE</li>
  <li><strong>Open With</strong> — one entry per detected / custom IDE (only when a repo is open)</li>
  <li><strong>Plugin Commands</strong> — registered via <code>arbor.command.register()</code></li>
</ul>
<p>Verb commands show a <code>›</code> chevron on the right to indicate they open a target picker. Leaf commands execute immediately. Conditional leaves (e.g. <em>Continue Rebase</em>, <em>Unstage All</em>) only show when the action is applicable.</p>

<h3>Phase 2 — Target picker</h3>
<p>Selecting a verb inserts a coloured chip at the start of the input (e.g. <kbd>⌥ Checkout ›</kbd>) and the list becomes the verb's target set. The input placeholder flips to match the verb's target — <em>"Filter branches…"</em>, <em>"Filter stashes…"</em>, <em>"Filter remotes…"</em>, etc. <kbd>Enter</kbd> runs the verb on the highlighted row; clicking the chip (or <kbd>Backspace</kbd> on empty input) removes it and returns to Phase 1.</p>
<p>Target kinds: <code>branch</code>, <code>tag</code>, <code>commit</code>, <code>file</code>, <code>stash</code>, <code>remote</code>, <code>tab</code>, <code>recent</code> (repository), <code>mr</code>, <code>theme</code>, <code>worktree</code>.</p>

<h2>Command reference</h2>
<h3>Branch verbs</h3>
<table class="shortcuts-table">
  <thead><tr><th>Command</th><th>Aliases</th><th>What it does</th></tr></thead>
  <tbody>
    <tr><td><code>Checkout</code></td><td><code>co</code>, <code>switch</code>, <code>sw</code>, <code>ck</code></td><td>Checks out the branch; opens the conflict modal if the workdir is dirty</td></tr>
    <tr><td><code>Merge</code></td><td>—</td><td>Merges the branch into HEAD</td></tr>
    <tr><td><code>Delete Branch</code></td><td><code>del</code>, <code>rm</code>, <code>delb</code></td><td>Removes the local branch (with confirm)</td></tr>
    <tr><td><code>Rename Branch</code></td><td><code>ren</code>, <code>mv</code></td><td>Opens the branch-rename modal with remote-rename toggle</td></tr>
    <tr><td><code>Push Branch</code></td><td><code>pushb</code></td><td>Pushes <code>refs/heads/&lt;branch&gt;</code> to <code>origin</code></td></tr>
    <tr><td><code>Focus Branch in Graph</code></td><td><code>focus</code>, <code>goto</code>, <code>go</code>, <code>show</code></td><td>Centers the graph on the branch HEAD</td></tr>
  </tbody>
</table>

<h3>Navigation &amp; Commit verbs</h3>
<table class="shortcuts-table">
  <thead><tr><th>Command</th><th>Aliases</th><th>Target</th><th>What it does</th></tr></thead>
  <tbody>
    <tr><td><code>Go to Tag</code></td><td><code>tag</code>, <code>tags</code></td><td>tag</td><td>Centers the graph on the tag target</td></tr>
    <tr><td><code>Go to Commit</code></td><td><code>commit</code>, <code>commits</code></td><td>commit</td><td>Full-text commit search (summary, author, hash) — min. 2 characters</td></tr>
    <tr><td><code>Blame File</code></td><td><code>blame</code>, <code>annotate</code></td><td>project-file</td><td>Opens the Git Blame modal for any file in the project — does <em>not</em> touch the File Tree sidebar</td></tr>
    <tr><td><code>Show Commits Touching File</code></td><td><code>file-history</code>, <code>log-file</code>, <code>history</code></td><td>project-file</td><td>Filters the graph by a file picked from the full project — does <em>not</em> open the File Tree sidebar</td></tr>
    <tr><td><code>Cherry-pick</code></td><td><code>cp</code>, <code>pick</code></td><td>commit</td><td>Applies the commit onto HEAD; routes to Stage on conflicts</td></tr>
    <tr><td><code>Revert Commit</code></td><td><code>rv</code></td><td>commit</td><td>Creates a new commit that undoes the target</td></tr>
    <tr><td><code>Reset Soft</code></td><td><code>rs</code></td><td>commit</td><td>Move HEAD only; keep index and workdir</td></tr>
    <tr><td><code>Reset Mixed</code></td><td>—</td><td>commit</td><td>Move HEAD + reset index; keep workdir</td></tr>
    <tr><td><code>Reset Hard</code></td><td><code>rh</code></td><td>commit</td><td><strong>Destructive</strong> — requires confirmation. Resets HEAD, index and workdir</td></tr>
    <tr><td><code>Create Branch Here</code></td><td><code>bf</code></td><td>commit</td><td>Opens the new-branch modal pre-seeded at the commit</td></tr>
    <tr><td><code>Create Tag</code></td><td><code>th</code>, <code>tag-here</code>, <code>create-tag</code></td><td>commit</td><td>Top entry <code>here</code> (selected by default — Enter creates a tag at HEAD); type a commit term to pre-seed the modal elsewhere</td></tr>
    <tr><td><code>Copy Commit SHA</code></td><td><code>sha</code></td><td>commit</td><td>Copies the full OID to the clipboard</td></tr>
  </tbody>
</table>

<h3>Stash / Tag / Remote verbs</h3>
<table class="shortcuts-table">
  <thead><tr><th>Command</th><th>Aliases</th><th>Target</th><th>What it does</th></tr></thead>
  <tbody>
    <tr><td><code>Apply Stash</code></td><td><code>apply</code></td><td>stash</td><td>Applies a stash without dropping it</td></tr>
    <tr><td><code>Pop Stash</code></td><td><code>pop</code></td><td>stash</td><td>Applies and drops the stash</td></tr>
    <tr><td><code>Drop Stash</code></td><td><code>drop</code></td><td>stash</td><td>Deletes the stash (with confirm)</td></tr>
    <tr><td><code>Delete Tag (local)</code></td><td><code>delt</code>, <code>rmt</code></td><td>tag</td><td>Removes the tag from this repo only (confirm modal)</td></tr>
    <tr><td><code>Delete Tag (local + origin)</code></td><td><code>delto</code>, <code>rmto</code></td><td>tag</td><td>Pushes a delete refspec to <code>origin</code> and removes the local ref (confirm modal)</td></tr>
    <tr><td><code>Push Tag</code></td><td><code>pusht</code></td><td>tag</td><td>Pushes <code>refs/tags/&lt;name&gt;</code> to origin</td></tr>
    <tr><td><code>Fetch from Remote</code></td><td><code>fr</code></td><td>remote</td><td>Fetches refs from a specific remote</td></tr>
    <tr><td><code>Pull from Remote</code></td><td><code>pr</code></td><td>remote</td><td>Pulls current branch from the chosen remote</td></tr>
    <tr><td><code>Push Branch to Remote</code></td><td><code>ptr</code></td><td>remote</td><td>Pushes the current branch to the chosen remote</td></tr>
  </tbody>
</table>

<h3>Tabs / Repository / Theme verbs</h3>
<table class="shortcuts-table">
  <thead><tr><th>Command</th><th>Aliases</th><th>Target</th><th>What it does</th></tr></thead>
  <tbody>
    <tr><td><code>Switch Tab</code></td><td><code>tab</code></td><td>tab</td><td>Activates the selected repo tab</td></tr>
    <tr><td><code>Close Tab</code></td><td><code>closet</code></td><td>tab</td><td>Closes the selected repo tab</td></tr>
    <tr><td><code>Open Recent Repository</code></td><td><code>recent</code>, <code>open</code></td><td>recent</td><td>Opens one of the recently-used repositories in a new tab</td></tr>
    <tr><td><code>Switch Theme</code></td><td><code>theme</code>, <code>colors</code></td><td>theme</td><td>Applies a built-in or custom theme (persists across restarts)</td></tr>
  </tbody>
</table>

<h3>Worktree verbs</h3>
<table class="shortcuts-table">
  <thead><tr><th>Command</th><th>Aliases</th><th>Target</th><th>What it does</th></tr></thead>
  <tbody>
    <tr><td><code>Worktree Info</code></td><td><code>wt</code>, <code>wtinfo</code>, <code>worktree</code></td><td>worktree</td><td>Opens the info panel for any worktree of the active project — same modal as the sidebar list, but reachable without expanding the <em>Worktrees</em> section</td></tr>
    <tr><td><code>Switch Worktree</code></td><td><code>wts</code>, <code>switch-wt</code></td><td>worktree</td><td>Swaps the active tab's context to the chosen worktree (or focuses an existing tab on that path) — same logic as double-clicking a row in the sidebar</td></tr>
  </tbody>
</table>
<p>Both verbs lazy-load the worktree list the first time they activate, then cache it for the lifetime of the palette open.</p>

<h3>Merge Request verbs</h3>
<table class="shortcuts-table">
  <thead><tr><th>Command</th><th>Aliases</th><th>Target</th><th>What it does</th></tr></thead>
  <tbody>
    <tr><td><code>View MR / PR Detail</code></td><td><code>mr</code>, <code>mrd</code>, <code>prd</code>, <code>mr-detail</code>, <code>pr-detail</code>, <code>view-mr</code>, <code>view-pr</code>, <code>open-mr</code>, <code>open-pr</code></td><td>mr</td><td>Opens the pull / merge request detail modal — same view you get from clicking a row in the MR sidebar</td></tr>
    <tr><td><code>Open Pull / Merge Request</code></td><td>—</td><td>—</td><td>Leaf action in the <strong>Merge Requests</strong> group — opens the create MR/PR modal</td></tr>
  </tbody>
</table>
<p>The MR list is fetched lazily the first time you enter an <code>mr</code>-target verb. It pulls <strong>all states</strong> in one shot — open, merged and closed — so the autocomplete can find an MR regardless of what filter the sidebar is showing, and is cached per repo tab so subsequent visits are instant. While the list is still loading a spinner is shown in the results area; refreshing the sidebar (force-refresh) also invalidates this cache so the next palette open re-fetches. Both verbs are hidden when the active repo's provider has pull / merge requests disabled (archived repo, fork mirror, branch-protection blocking PRs, …).</p>

<h3>Deep Link verbs</h3>
<p>Build a shareable <code>arbor://</code> URL and copy it to the clipboard. The active tab's first remote is embedded as <code>?url=</code>, so the link resolves on any machine that has access to the same remote. If the repository has no remote configured, the palette toasts a warning rather than producing a non-shareable link — see the <em>Deep Links</em> doc page for the full URL schema.</p>
<table class="shortcuts-table">
  <thead><tr><th>Command</th><th>Aliases</th><th>Target</th><th>Produces</th></tr></thead>
  <tbody>
    <tr><td><code>Copy arbor:// Link to Commit</code></td><td><code>linkc</code>, <code>dl-commit</code></td><td>commit</td><td><code>arbor://commit/&lt;sha&gt;?url=&lt;remote&gt;</code></td></tr>
    <tr><td><code>Copy arbor:// Link to Checkout Branch</code></td><td><code>linkb</code>, <code>dl-checkout</code></td><td>branch</td><td><code>arbor://branch/&lt;name&gt;?url=&lt;remote&gt;&amp;checkout=1</code></td></tr>
    <tr><td><code>Copy arbor:// Link to Branch Worktree</code></td><td><code>linkw</code>, <code>dl-worktree</code></td><td>branch</td><td><code>arbor://branch/&lt;name&gt;?url=&lt;remote&gt;&amp;worktree=1</code></td></tr>
    <tr><td><code>Copy arbor:// Link to MR</code></td><td><code>linkmr</code>, <code>dl-mr</code></td><td>mr</td><td><code>arbor://mr/open/&lt;number&gt;?url=&lt;remote&gt;</code></td></tr>
  </tbody>
</table>
<p>The <em>Open Repository</em> variant has no target, so it lives as a leaf entry under <strong>Copy</strong> (<em>Copy arbor:// Link to Open Repository</em>) and produces <code>arbor://repo/open?url=&lt;remote&gt;</code>.</p>

<h2>Auto-promote shortcut</h2>
<p>Typing a verb name (or any alias) followed by a space — or a colon — promotes it to a chip immediately and keeps whatever you typed after as the target filter. This lets power users skip the list entirely:</p>
<ul>
  <li><code>co main</code> → chip <kbd>Checkout</kbd>, filter <em>main</em></li>
  <li><code>merge develop</code> → chip <kbd>Merge</kbd>, filter <em>develop</em></li>
  <li><code>tag:v1</code> → chip <kbd>Go to Tag</kbd>, filter <em>v1</em></li>
  <li><code>rm feature/old</code> → chip <kbd>Delete Branch</kbd>, filter <em>feature/old</em></li>
  <li><code>cp fix</code> → chip <kbd>Cherry-pick</kbd>, filter commits containing <em>fix</em></li>
  <li><code>apply WIP</code> → chip <kbd>Apply Stash</kbd>, filter stashes containing <em>WIP</em></li>
  <li><code>tab:docs</code> → chip <kbd>Switch Tab</kbd>, filter tabs whose name contains <em>docs</em></li>
  <li><code>wt feature/api</code> → chip <kbd>Worktree Info</kbd>, filter worktrees whose branch contains <em>feature/api</em></li>
  <li><code>linkc bug-fix</code> → chip <kbd>Copy arbor:// Link to Commit</kbd>, filter commits matching <em>bug-fix</em></li>
</ul>
<p>The verb chip is always visible, so there is no hidden state: the palette shows exactly what <kbd>Enter</kbd> will do.</p>

<h2>Destructive actions &amp; confirmations</h2>
<p>A handful of commands require explicit confirmation because they cannot be undone or affect stashed work:</p>
<ul>
  <li><em>Delete Branch</em>, <em>Drop Stash</em>, <em>Unlink from "&lt;link&gt;"</em>, <em>Discard All Changes</em> — themed confirm modal with Enter-to-confirm</li>
  <li><em>Delete Tag (local)</em> and <em>Delete Tag (local + origin)</em> — confirm modal that spells out the scope</li>
  <li><em>Reset Hard</em> — confirm modal that lists the target SHA being reset to</li>
  <li><em>Undo Last Commit</em> — confirm modal that shows the parent SHA HEAD will move to</li>
</ul>

<h2>Open With — launching an IDE</h2>
<p>The <strong>Open With</strong> section is populated from your IDE configuration in <em>Settings → IDE Integration</em>:</p>
<ul>
  <li>All built-in IDEs detected at startup (or with a custom <em>executable path</em> set) are listed automatically</li>
  <li>Custom IDEs added in settings appear alongside the built-ins</li>
  <li>The IDE is launched <strong>detached</strong> — closing Arbor does not close the IDE</li>
</ul>
<p>For a quick one-click launch with your default IDE, use the <em>Open in IDE</em> entry in the <strong>Actions</strong> section. For a specific IDE, pick from <strong>Open With</strong>.</p>

<h2>Ghost-text autocompletion</h2>
<p>As you type, the palette shows a dimmed ghost suffix in the input box when the first result title starts with your current query. Press <kbd>Tab</kbd> to expand the input to the full suggested title, or keep typing to refine.</p>

<h2>Fuzzy scoring</h2>
<p>Each item is assigned a score based on how well its title and subtitle match the query:</p>
<ul>
  <li>Exact match → 100</li>
  <li>Prefix match → 85</li>
  <li>Word-boundary match → 70</li>
  <li>Substring match → 55</li>
  <li>Fuzzy (all characters present in order) → 30</li>
  <li>No match → hidden</li>
</ul>
<p>Sections with no matching items are hidden entirely.</p>

<h2>Plugin commands</h2>
<p>Plugin-registered commands appear in the palette under the <strong>Plugin Commands</strong> section. They fire <code>command:&lt;id&gt;</code> on the owning plugin when selected.</p>

{#if pluginCommands.length > 0}
  <p>Currently registered by enabled plugins:</p>
  <table class="shortcuts-table">
    <thead><tr><th>Title</th><th>Description</th><th>Plugin</th><th>Action</th></tr></thead>
    <tbody>
      {#each pluginCommands as cmd (cmd.plugin_name + ':' + cmd.item_id)}
        <tr>
          <td>{cmd.title}</td>
          <td>{cmd.description || '—'}</td>
          <td><code>{cmd.plugin_name}</code></td>
          <td><code>command:{cmd.item_id}</code></td>
        </tr>
      {/each}
    </tbody>
  </table>
{:else}
  <p class="hint">No plugin commands registered. Enable a plugin that calls <code>arbor.command.register()</code> to populate this list.</p>
{/if}

<h2>Plugin API — <code>arbor.command</code></h2>
<p>Plugins can register and remove command palette entries at runtime using <code>arbor.command.register()</code> and <code>arbor.command.unregister()</code>. Call <code>register</code> during <code>on_plugin_load</code> so the commands are available as soon as the plugin loads.</p>
<pre class="language-lua">{@html highlight(`-- Register a command palette entry
-- Fields: id (required), title (required), description?, icon?, group?
arbor.command.register({
  id          = "run-tests",
  title       = "Run Tests",
  description = "Execute the test suite",
  icon        = "Play",          -- Lucide icon name
  group       = "My Plugin",
})

-- Handle execution: the action name is  "command:<id>"
arbor.events.on("command:run-tests", function(_ctx)
  arbor.job.spawn({ id = "tests", cmd = "cargo", args = {"test"} })
end)

-- Remove a command at runtime (e.g. when a feature is unavailable)
arbor.command.unregister("run-tests")`, '.lua')}</pre>

<h3>Fields</h3>
<table class="shortcuts-table">
  <thead><tr><th>Field</th><th>Type</th><th>Description</th></tr></thead>
  <tbody>
    <tr><td><code>id</code></td><td>string</td><td>Unique identifier within the plugin. Used as the action name <code>command:&lt;id&gt;</code>.</td></tr>
    <tr><td><code>title</code></td><td>string</td><td>Display title shown in the palette.</td></tr>
    <tr><td><code>description</code></td><td>string?</td><td>Subtitle shown below the title (e.g. short description or plugin name).</td></tr>
    <tr><td><code>icon</code></td><td>string?</td><td>Lucide icon name (e.g. <code>"Play"</code>, <code>"GitBranch"</code>, <code>"Settings"</code>). Defaults to <code>"Zap"</code> if omitted.</td></tr>
    <tr><td><code>group</code></td><td>string?</td><td>Optional category label; currently used for internal grouping only.</td></tr>
  </tbody>
</table>

<h3>Hook convention</h3>
<p>When the user selects a plugin command the palette fires <code>fire_plugin_action(plugin_name, "command:&lt;id&gt;", "&#123;&#125;")</code>. Register the handler with <code>arbor.events.on("command:&lt;id&gt;", fn)</code> — the same mechanism used for activity-bar actions and keybindings.</p>

<h3>Full example</h3>
<pre class="language-lua">{@html highlight(`-- plugins/my-plugin/main.lua

arbor.events.on("on_plugin_load", function(_ctx)
  arbor.command.register({
    id    = "open-dashboard",
    title = "Open Dashboard",
    icon  = "LayoutPanelLeft",
    group = "My Plugin",
  })
  arbor.command.register({
    id          = "deploy-prod",
    title       = "Deploy to Production",
    description = "Runs deploy.sh on the active repo",
    icon        = "Upload",
    group       = "My Plugin",
  })
end)

arbor.events.on("command:open-dashboard", function(_ctx)
  arbor.notify{ message = "Dashboard opened!", level = "info" }
end)

arbor.events.on("command:deploy-prod", function(_ctx)
  local repo = arbor.repo.current()
  if not repo then
    arbor.notify{ title = "Deploy", message = "No active repository", level = "error" }
    return
  end
  arbor.job.spawn({
    id   = "deploy",
    cmd  = "bash",
    args = { repo .. "/deploy.sh" },
  })
end)`, '.lua')}</pre>
