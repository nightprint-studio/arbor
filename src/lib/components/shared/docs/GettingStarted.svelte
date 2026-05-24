<script lang="ts">
  // Doc pages otherwise have no script — this single block lets us wire
  // in the welcome-tour CTA and gives us the imports we need to use the
  // shared Callout / Kbd widgets so this page stays consistent with the
  // rest of the app (keybindings here update live when the user remaps).
  import Callout from '$lib/components/shared/ui/Callout.svelte';
  import Kbd     from '$lib/components/shared/internal/Kbd.svelte';

  function launchTour() {
    window.dispatchEvent(new CustomEvent('arbor:open-onboarding'));
  }
</script>

<span class="eyebrow">Welcome</span>
<h1>Getting Started</h1>

<p class="doc-lead">Arbor is a Git GUI built on Tauri, Rust, and Svelte 5 — no Electron, no Node.js runtime overhead. Fast, keyboard-driven, extensible via Lua plugins.</p>

<div class="stat-row">
  <div class="stat">
    <div class="stat-value">Tauri 2</div>
    <div class="stat-label">Native shell</div>
  </div>
  <div class="stat">
    <div class="stat-value">Rust</div>
    <div class="stat-label">libgit2 backend</div>
  </div>
  <div class="stat">
    <div class="stat-value">Svelte 5</div>
    <div class="stat-label">Runes UI</div>
  </div>
  <div class="stat">
    <div class="stat-value">Lua 5.4</div>
    <div class="stat-label">Plugin runtime</div>
  </div>
</div>

<Callout variant="tip" title="Quick Start">
  Press <Kbd action="open_repo" />, select any folder containing a <code>.git</code> directory, and the commit graph loads instantly.
</Callout>

<Callout variant="info" title="Welcome tour">
  Re-open the first-run walkthrough any time —
  <button class="tour-link" type="button" onclick={launchTour}>Launch the welcome tour</button>
  — or find it in the Command Palette under <em>Welcome Tour</em>.
</Callout>

<h2>Opening a repository</h2>
<ol class="step-list">
  <li>Press <Kbd action="open_repo" /> or click the <strong>+</strong> button in the tab bar</li>
  <li>Select the folder containing your <code>.git</code> directory</li>
  <li>The commit graph, branches, tags, and stashes all load automatically</li>
</ol>
<p>If the selected folder has no <code>.git</code> directory, Arbor will offer to <strong>initialize a new repository</strong> — see the <em>Initialize Repository</em> section.</p>

<h2>Multiple tabs</h2>
<p>Open multiple repositories simultaneously in separate tabs.</p>
<table class="shortcuts-table">
  <thead><tr><th>Action</th><th>Shortcut</th></tr></thead>
  <tbody>
    <tr><td>Next tab</td><td><Kbd action="next_tab" /></td></tr>
    <tr><td>Previous tab</td><td><Kbd action="prev_tab" /></td></tr>
    <tr><td>Close tab</td><td><Kbd action="close_tab" /></td></tr>
    <tr><td>Recent repos quick-switch</td><td><Kbd action="open_recent" /></td></tr>
  </tbody>
</table>
<p>Right-click any tab for more options: reveal in explorer, copy path, rename, close others.</p>

<h2>Interface overview</h2>
<div class="feature-grid">
  <div class="feature-card">
    <div class="fc-title">Activity Bar</div>
    <div class="fc-desc">Narrow icon rail on the left. Top icons toggle sidebar panels; bottom icons toggle the detail and stage panels. Plugin actions can appear here too.</div>
  </div>
  <div class="feature-card">
    <div class="fc-title">Sidebar <Kbd action="toggle_sidebar" /></div>
    <div class="fc-desc">Local/remote branches, tags, and stashes. Double-click a branch to check it out. Ahead/behind counts shown inline.</div>
  </div>
  <div class="feature-card">
    <div class="fc-title">Commit Graph</div>
    <div class="fc-desc">SVG lane graph with virtual scrolling — handles repositories of any size without performance degradation. Search with <Kbd action="search" />.</div>
  </div>
  <div class="feature-card">
    <div class="fc-title">Detail Panel</div>
    <div class="fc-desc">Commit metadata, changed file list, and syntax-highlighted diff viewer. Dockable at the bottom or the right side (see Settings).</div>
  </div>
  <div class="feature-card">
    <div class="fc-title">Stage Area <Kbd action="stage_view" /></div>
    <div class="fc-desc">Stage/unstage files, write commit messages, manage stashes. Supports hunk-level and line-level partial staging.</div>
  </div>
  <div class="feature-card">
    <div class="fc-title">Status Bar</div>
    <div class="fc-desc">Current branch, ahead/behind indicators, repo path, and quick-access buttons for fetch, notifications, docs, and settings.</div>
  </div>
</div>

<h2>Command Palette</h2>
<p>Press <Kbd action="command_palette" /> to open the Command Palette — a unified search overlay for actions, branches, commits, and plugin commands. Everything is reachable without touching the mouse.</p>

<style>
  .tour-link {
    background: none;
    border: none;
    padding: 0;
    font: inherit;
    color: var(--accent);
    cursor: pointer;
    text-decoration: underline;
    text-underline-offset: 2px;
  }
  .tour-link:hover { color: var(--accent-hover); }
</style>
