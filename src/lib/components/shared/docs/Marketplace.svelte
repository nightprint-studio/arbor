<h1>Marketplace</h1>

<p class="doc-lead">
  The Marketplace is a one-click browser for plugins and themes hosted in the
  <code>arbor-extensions</code> registry on GitHub, plus any custom git URL you choose
  to add. Open it from the <strong>Browse</strong> button at the top of the Plugin
  Manager.
</p>

<h2>How it works</h2>
<p>
  Arbor never bundles plugin metadata in the binary. The Marketplace fetches a
  small <code>index.json</code> pointer file from
  <code>github.com/nightprint-studio/arbor-extensions</code>, then resolves each
  entry's <code>plugin.toml</code> directly from the source repo. This way:
</p>
<ul>
  <li>Authors update one file (their own <code>plugin.toml</code>) — never the registry.</li>
  <li>The registry stays a tiny list of pointers — easy to PR-review.</li>
  <li>Icons, docs, screenshots come straight from the repo, always in sync with the code.</li>
</ul>

<h2>Internal vs external entries</h2>
<p>
  Registry entries come in two shapes — both surface as <strong>Community</strong> because
  both are vetted via PR review on <code>arbor-extensions</code>:
</p>
<ul>
  <li>
    <strong>Internal</strong> — the plugin/theme lives inside the registry repo itself
    (<code>{'{ "subpath": "plugins/foo" }'}</code>). The historical layout for plugins
    maintained alongside Arbor.
  </li>
  <li>
    <strong>External</strong> — the entry points at a third-party GitHub repo
    (<code>{'{ "repo": "https://github.com/author/their-plugin", "ref": "v1.2", "pinned_sha": "abc1234…" }'}</code>).
    The author keeps ownership of the code in their own repo; the registry just stores
    a pointer.
  </li>
</ul>
<p>
  External entries that omit <code>pinned_sha</code> get an <strong>Unpinned</strong> badge
  in the detail header — a hint that the entry follows a moving ref (branch or tag) and
  could change underneath you when Arbor refetches. Pinning to a commit SHA means the
  Marketplace only ever installs the exact code that was reviewed when the entry was
  merged; bumping the pin requires a fresh PR.
</p>

<h2>Installing a plugin</h2>
<ol>
  <li>Click a row in the catalog to open its detail pane.</li>
  <li>Review the requested permissions in the body.</li>
  <li>Hit <strong>Install</strong>. A confirmation modal lists the same permissions in
      human-readable form — read carefully and confirm.</li>
  <li>Arbor downloads the GitHub zipball, extracts it to
      <code>~/.config/arbor/marketplace_plugins/&lt;name&gt;/</code>, and reloads the plugin
      host. The plugin lands <em>disabled by default</em>.</li>
  <li>Toggle <strong>Enabled</strong> in the detail pane when you're ready to use it.</li>
</ol>

<p>
  Plugins installed through the Marketplace get a small <strong>Marketplace</strong>
  badge in the Plugin Manager so they're visually distinct from dev / hand-copied
  plugins. The two pools live in separate directories and never collide — if a name
  collision happens, the dev plugin wins and the marketplace shadow is logged + skipped.
</p>

<h2>Custom sources</h2>
<p>
  Click <strong>Add custom source</strong> in the modal footer to point Arbor at any
  GitHub repo. The resolver detects the layout automatically:
</p>
<ol>
  <li>If a <code>subpath</code> is supplied → fetches <code>&lt;subpath&gt;/plugin.toml</code> (subpath mode).</li>
  <li>Else, looks for <code>plugin.toml</code> at the repo root → single plugin (root mode).</li>
  <li>Else, looks for an <code>index.json</code> at the root → multi-plugin registry (mirror mode).</li>
</ol>
<p>
  Custom sources are persisted in <code>~/.config/arbor/user_registry.toml</code> and
  re-resolved every time the catalog refreshes. Installed plugins from a custom source
  survive removing the source — they're tracked independently in the install ledger.
</p>

<h2>Updates</h2>
<p>
  When the catalog version is newer than the installed version, the row shows a
  yellow <strong>Update</strong> pill and the detail header switches to
  <code>v1.2 → v1.3</code>. The <strong>Update to v…</strong> button re-runs the
  install path (overwrites the existing folder + reloads the host). You're shown the
  permission confirmation again in case the new version asks for more.
</p>

<h2>Auto-refresh scheduler</h2>
<p>
  Arbor runs a small background task that polls the marketplace cache and re-fetches
  when it ages past your configured interval. Tune it from
  <strong>Settings → Tools → Marketplace</strong>:
</p>
<ul>
  <li><strong>Enable scheduler</strong> — master switch. When off, the catalog only
      refreshes when you hit the <strong>Refresh</strong> button in the modal footer
      (or use <em>Settings → Tools → Marketplace → Refresh now</em>).</li>
  <li><strong>Refresh interval</strong> — 1h to 7d. How long the cache may go without
      a refresh before the scheduler re-fetches.</li>
  <li><strong>Poll cadence</strong> — 1 to 60 minutes. How often the scheduler wakes
      up to check the cache age. 10 minutes is the sensible default — finer values
      just burn cycles checking a multi-hour interval, larger values lag behind
      settings changes.</li>
</ul>
<p>
  The fetch itself hits <code>raw.githubusercontent.com</code> for a handful of small
  files (typically &lt;200 KB total). Even hourly refreshes are negligible bandwidth.
</p>

<h2>Files on disk</h2>
<ul>
  <li><code>~/.config/arbor/marketplace_plugins/&lt;name&gt;/</code> — extracted plugin folders.</li>
  <li><code>~/.config/arbor/marketplace_installed.json</code> — install ledger
      (name, version, repo, ref, resolved SHA, install path, enabled state).</li>
  <li><code>~/.config/arbor/marketplace_cache.json</code> — last-fetched community
      catalog. 1h TTL by default.</li>
  <li><code>~/.config/arbor/marketplace_custom.json</code> — last-resolved custom
      sources. Refreshed on each network fetch.</li>
  <li><code>~/.config/arbor/user_registry.toml</code> — your custom source pointers.</li>
  <li><code>~/.config/arbor/themes/&lt;id&gt;.json</code> — installed marketplace themes
      (same dir the host's theme picker reads).</li>
</ul>
<p>
  Dev builds use <code>-dev</code> suffixes (e.g. <code>marketplace_plugins-dev/</code>)
  so a side-by-side prod Arbor's data stays untouched.
</p>
