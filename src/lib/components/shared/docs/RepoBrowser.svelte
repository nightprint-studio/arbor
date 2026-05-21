<script lang="ts">
  import { languageEntries } from '$lib/utils/language-colours';
  const legend = languageEntries();
</script>

<h1>Repository Browser</h1>

<p class="doc-lead">Browse, preview, and clone repositories hosted on GitHub or GitLab — without leaving Arbor. The Repository Browser gives you a full file-tree explorer with syntax-highlighted previews and one-click cloning.</p>

<h2>Opening the Repository Browser</h2>
<ul>
  <li>Click the <strong>Repository Browser</strong> button in the hamburger menu (☰ → Repository Browser)</li>
  <li>Press <kbd>Ctrl+Shift+R</kbd></li>
</ul>
<div class="callout tip">
  <strong>Requires connected accounts.</strong>
  Go to <em>Settings → Git &amp; Integrations</em> to add a GitHub or GitLab token before using the browser.
</div>

<h2>Layout</h2>
<div class="feature-grid">
  <div class="feature-card">
    <div class="fc-title">Left panel — Repository list</div>
    <div class="fc-desc">Shows all repositories for the selected account, grouped by namespace (organisation / group). Supports live search. A green dot marks repos already open as a local tab.</div>
  </div>
  <div class="feature-card">
    <div class="fc-title">Right panel — File tree &amp; preview</div>
    <div class="fc-desc">Repo metadata (description, language, stars, size, last update) in the header. Below it: a breadcrumb navigator + file-tree. Click a file to open a syntax-highlighted preview.</div>
  </div>
</div>

<h2>Switching accounts</h2>
<p>The account selector at the top of the left panel lets you switch between connected GitHub and GitLab accounts. Each account shows its avatar and username. The dropdown lists all configured providers — click one to reload the repository list for that account.</p>

<h2>Browsing files</h2>
<ul>
  <li>Click a <strong>directory</strong> to navigate into it. The breadcrumb updates automatically.</li>
  <li>Click <strong>← ..</strong> (back button) or any breadcrumb segment to go up.</li>
  <li>Click a <strong>file</strong> to open it in the preview pane below the tree.</li>
  <li>The preview shows syntax-highlighted code, images (inline), or a download prompt for binary files.</li>
</ul>

<h2>File preview actions</h2>
<table class="shortcuts-table">
  <thead><tr><th>Action</th><th>Description</th></tr></thead>
  <tbody>
    <tr><td>Copy</td><td>Copies the raw file content to the clipboard (no line numbers).</td></tr>
    <tr><td>Download</td><td>Saves the file to a folder you choose via the native file picker.</td></tr>
    <tr><td>Close preview</td><td>Dismisses the preview pane; the file tree expands back to full height.</td></tr>
  </tbody>
</table>

<h2>Cloning a repository</h2>
<p>Select a repository from the list, then click the <strong>Clone</strong> button in the repo bar. A folder picker lets you choose the parent directory — Arbor appends the repo name automatically. Once cloning completes, the repo opens as a new tab.</p>
<p>If the repository is already open locally, the <strong>Clone</strong> button is replaced by <strong>Open Tab</strong>, which switches directly to the existing tab.</p>

<h2>Opening in browser</h2>
<p>The external-link icon (<strong>↗</strong>) in the repo bar opens the repository's web page (GitHub / GitLab) in your default browser.</p>

<h2>Sidebar toggle</h2>
<p>The panel-toggle button in the header collapses the left repository list, giving the file tree and preview more horizontal space. Click it again to restore the list.</p>

<h2>Repo list cache</h2>
<p>"List all repos" can take 30s+ on accounts with hundreds of projects. Arbor caches the result in <code>localStorage</code> per provider so reopening the modal is instant.</p>
<ul>
  <li>The strip below the search box shows when the list was last fetched (<em>Cached · 4m ago</em> / <em>Updated · just now</em>) and exposes a <strong>Refresh</strong> button that bypasses the cache.</li>
  <li>If the cache is past its TTL but still present, Arbor shows the stale list immediately and refetches in the background — the strip updates to <em>Updated</em> once the fresh list arrives.</li>
  <li>Tune the TTL (default 10 minutes) or wipe the cache from <em>Settings → Cache → Repository Browser</em>.</li>
  <li>Set the TTL to <code>0</code> to disable caching entirely (every open re-fetches).</li>
</ul>
<div class="callout tip">
  <strong>Backend speed-ups.</strong>
  Pages 2..N of the GitHub/GitLab repo list are now fetched concurrently
  (capped by the API's own rate limit). GitLab's slow <code>statistics=true</code>
  flag was dropped — the list view doesn't display repo size, so paying for
  it on every open isn't worth it.
</div>

<h2>Language colours</h2>
<p>Each repository row shows a coloured dot next to the last-update time, indicating the repo's primary language. The palette mirrors GitHub's Linguist colours so the dots match what you see on github.com. Click <strong>Legend</strong> at the bottom of the repo list to open the same legend inline.</p>
<div class="lang-legend-grid">
  {#each legend as [name, colour]}
    <div class="lang-legend-item">
      <span class="lang-legend-dot" style="background: {colour}"></span>
      <span>{name}</span>
    </div>
  {/each}
</div>
<div class="callout tip">
  Languages not in the palette fall back to a neutral grey dot. The dot is hidden entirely if the provider didn't return a primary language for the repo.
</div>

<style>
  .lang-legend-grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(160px, 1fr));
    gap: 6px 14px;
    margin: 12px 0 16px;
  }
  .lang-legend-item {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 12px;
    color: var(--text-secondary);
  }
  .lang-legend-dot {
    width: 10px;
    height: 10px;
    border-radius: 50%;
    flex-shrink: 0;
    box-shadow: inset 0 0 0 1px rgba(0, 0, 0, 0.15);
  }
</style>

<h2>Supported providers</h2>
<table class="shortcuts-table">
  <thead><tr><th>Provider</th><th>Requirements</th></tr></thead>
  <tbody>
    <tr><td>GitHub</td><td>Personal access token with <code>repo</code> scope (Settings → Git &amp; Integrations → GitHub)</td></tr>
    <tr><td>GitLab</td><td>Personal access token with <code>read_api</code> + <code>read_repository</code> scopes; supports self-hosted instances</td></tr>
  </tbody>
</table>
