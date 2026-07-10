<script lang="ts">
  import Callout from '$lib/components/shared/ui/Callout.svelte';
</script>

<h1>Settings Sync</h1>

<p class="doc-lead">
  <strong>Settings Sync</strong> mirrors your corvus workspaces, settings, installed-mod list
  and light plugin data to a <strong>private GitHub repository</strong>, so a second machine
  picks up where you left off. It is backed by a repo you own — created automatically the first
  time you enable it — and pushes on its own schedule in the background.
</p>

<h2>What gets synced</h2>
<div class="feature-grid two-col">
  <div class="feature-card">
    <div class="fc-title">Workspaces &amp; repos</div>
    <div class="fc-desc">Your workspaces and their groups. Repositories are identified by their
      <em>remote URL</em>, never by an absolute path.</div>
  </div>
  <div class="feature-card">
    <div class="fc-title">Settings</div>
    <div class="fc-desc">UI settings (theme, keyboard inputs, animations, activity bar) and the
      corvus git preferences.</div>
  </div>
  <div class="feature-card">
    <div class="fc-title">Mod list</div>
    <div class="fc-desc">The names, versions and enable state of your installed plugins. A second
      machine re-installs them from the Marketplace.</div>
  </div>
  <div class="feature-card">
    <div class="fc-title">Light plugin data</div>
    <div class="fc-desc">Each plugin's small global settings (e.g. saved compile/run commands).
      Files over the size cap are skipped.</div>
  </div>
</div>

<Callout variant="info">
  What is <strong>never</strong> synced: repository paths, credentials (they live in the OS
  keyring — re-authenticate per machine), and heavy caches/indexes (they rebuild locally).
</Callout>

<h2>Enabling</h2>
<p>
  Open <strong>Settings → Git → Settings Sync</strong>. A connected <strong>GitHub</strong>
  account is required (connect it under <strong>Settings → Access → Git</strong> first). Choose
  the provider (GitHub), optionally give a repository name — leave it blank to use or adopt the
  default <code>arbor-corvus-sync</code> — and click <strong>Enable &amp; push</strong>. If the
  repo doesn't exist it is created <strong>private</strong>, and a first push runs immediately.
</p>

<h2>Auto-push</h2>
<p>
  Once enabled, a background driver watches for changes and pushes them to the sync repo,
  batched by the configured <strong>push interval</strong> (minimum 30 seconds). Nothing is
  pushed unless something actually differs from what was last sent. You can push at any time
  with <strong>Push now</strong> or the <em>Sync: Push now</em> Command Palette action; the
  section shows the last push time and whether changes are pending.
</p>

<h2>Pull &amp; merge</h2>
<p>
  <strong>Pull &amp; merge…</strong> (also <em>Sync: Pull &amp; merge…</em> in the Command
  Palette) reads the remote bundle and shows a <strong>per-item review</strong>: for every
  workspace, settings group and plugin-data entry you choose <em>Keep local</em> or
  <em>Use remote</em>, and you can apply the remote mods' enable states. Unchanged items default
  to keeping your local copy.
</p>
<p>
  Because repos are matched by remote URL, a synced workspace re-links to the same repositories
  on the new machine automatically. Repositories referenced by a synced workspace but not present
  locally are listed so you can clone or locate them afterwards.
</p>

<Callout variant="warning">
  Applying remote UI settings updates theme and motion live; other changes may need the window
  to be re-opened to take full effect.
</Callout>

<h2>What to sync</h2>
<p>
  Each of the four categories has its own toggle in the section, so you can, for example, sync
  workspaces and settings but leave plugin data out. The plugin-data size cap keeps large blobs
  out of the bundle regardless.
</p>
