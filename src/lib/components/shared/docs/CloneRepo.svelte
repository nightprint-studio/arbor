<script lang="ts">
  import Callout from '$lib/components/shared/ui/Callout.svelte';
  import Kbd     from '$lib/components/shared/internal/Kbd.svelte';
</script>

<h1>Clone Repository</h1>

<p class="doc-lead">Clone any remote repository directly from the UI — no terminal required. Arbor handles authentication, progress tracking, and opens the repo automatically when done.</p>

<h2>Opening the clone dialog</h2>
<ul>
  <li>Click <strong>Clone</strong> on the welcome screen (shown when no repo is open)</li>
  <li>Click the <strong>+</strong> tab button → <strong>Clone Repository</strong></li>
  <li>Press <Kbd action="clone_repo" /></li>
</ul>

<h2>Clone options</h2>
<div class="feature-grid">
  <div class="feature-card">
    <div class="fc-title">Repository URL</div>
    <div class="fc-desc">Any valid Git URL: HTTPS (<code>https://github.com/…</code>), SSH (<code>git@github.com:…</code>), or a local path.</div>
  </div>
  <div class="feature-card">
    <div class="fc-title">Destination folder</div>
    <div class="fc-desc">The parent directory for the clone. Arbor appends the repo name automatically — cloning <code>my-repo</code> into <code>~/projects</code> creates <code>~/projects/my-repo</code>.</div>
  </div>
  <div class="feature-card">
    <div class="fc-title">Branch</div>
    <div class="fc-desc">Optionally specify a branch to check out after cloning. Leave empty to use the remote's default branch.</div>
  </div>
</div>

<h2>Authentication</h2>
<p>Arbor uses credentials from the OS keyring — the same store used by Settings → Git &amp; Integrations:</p>
<ul>
  <li><strong>HTTPS</strong> — username/password or personal access token stored against the hostname</li>
  <li><strong>SSH</strong> — uses the SSH agent or the key from <code>~/.ssh/config</code></li>
</ul>
<Callout variant="tip" title="Missing credentials?">
  If the clone fails with an authentication error, add your credentials in <em>Settings → Git &amp; Integrations → Credentials</em> and retry.
</Callout>

<h2>Progress &amp; completion</h2>
<p>A progress bar tracks the clone operation (objects received, deltas resolved). The dialog stays open until the clone finishes or fails. On success, the repository opens automatically as a new tab.</p>

<h2>Common errors</h2>
<table class="shortcuts-table">
  <thead><tr><th>Error</th><th>Likely cause</th></tr></thead>
  <tbody>
    <tr>
      <td>Authentication failed</td>
      <td>Wrong or missing credentials. Add/update in Settings → Credentials.</td>
    </tr>
    <tr>
      <td>Repository not found</td>
      <td>The URL is wrong, or the repo is private and your token doesn't have access.</td>
    </tr>
    <tr>
      <td>Destination already exists</td>
      <td>The target folder is non-empty. Choose a different path or remove the existing folder first.</td>
    </tr>
  </tbody>
</table>
