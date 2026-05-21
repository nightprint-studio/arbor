<script lang="ts">
  import { RefreshCw, RotateCcw, ExternalLink } from 'lucide-svelte';
</script>

<h1>Pipelines — CI / CD</h1>
<p>The <strong>CI / CD</strong> tab in the Pipelines panel connects to GitHub Actions or GitLab CI and shows real pipeline runs fetched directly from the API. This works for any repository whose remote URL points to <code>github.com</code> or a GitLab instance.</p>

<h2>Authentication</h2>
<p>An OAuth token is required. Connect your account in <strong>Settings → Access → Git &amp; Integrations</strong> before using the CI tab:</p>
<ul>
  <li><strong>GitHub Actions</strong> — connect your GitHub account via Device Flow. Arbor requests the <code>repo</code> + <code>read:user</code> scopes.</li>
  <li><strong>GitLab CI</strong> — connect via GitLab Device Flow. Arbor requests the <code>api</code> + <code>read_user</code> scopes. Self-hosted instances use a host-based credential stored with <strong>Settings → Additional Git Credentials</strong>.</li>
</ul>
<p>If no token is found, the CI tab shows a banner directing you to Settings rather than an error.</p>

<h2>CI run list</h2>
<p>Each row in the CI run list shows:</p>
<ul>
  <li>A <strong>status pill</strong> (Passed / Failed / Running / Cancelled / Pending) with colour coding.</li>
  <li>The <strong>wall-clock duration</strong> (computed from API timestamps).</li>
  <li>The <strong>workflow / pipeline name</strong> and its provider ID.</li>
  <li>The <strong>branch chip</strong> (accent colour) and short <strong>commit SHA</strong>.</li>
  <li>A human-readable <strong>time-ago</strong> label.</li>
</ul>
<p>Click anywhere on a run card to open the <strong>Pipeline Detail</strong> modal.</p>

<h2>Pipeline detail modal</h2>
<p>Clicking a run opens a full-screen modal showing:</p>
<ul>
  <li>Header: provider icon, run name, branch/commit/duration chips, status badge.</li>
  <li>A <strong>stage/job graph</strong> — horizontal columns, one per stage (GitLab) or "Jobs" (GitHub). Each column lists job cards with their status icon, name, and duration. Clicking a job card opens its log page in the browser.</li>
  <li>Jobs with <code>allow_failure: true</code> are shown slightly dimmed with an <strong>!</strong> badge when they fail.</li>
  <li><strong>Re-run</strong> and <strong>Open in browser</strong> buttons in the modal header.</li>
</ul>
<p>For GitLab, jobs are grouped by their native <code>stage</code> name. For GitHub, all jobs appear in a single "Jobs" column since GitHub Actions does not expose a first-class stage concept in the jobs API.</p>

<h2>Creating a new pipeline run</h2>
<p>Click the <strong>Run</strong> button in the CI / CD header (only visible when a token is configured) to open the <em>New Pipeline Run</em> modal:</p>
<ul>
  <li><strong>Branch</strong> — dropdown pre-filled with the current HEAD branch. All local branches are listed.</li>
  <li><strong>Workflow</strong> (GitHub only) — dropdown listing active workflows that have <code>on: workflow_dispatch</code> configured. If no dispatch-enabled workflows are found, a hint is shown.</li>
  <li><strong>Variables</strong> — dynamic key/value table. Add as many variables as needed; blank-key rows are ignored on submit. For GitLab these become <code>env_var</code> variables; for GitHub they become <code>workflow_dispatch</code> inputs.</li>
</ul>
<p>After clicking <strong>Run Pipeline</strong>:</p>
<ul>
  <li><strong>GitLab</strong> — the new pipeline is created synchronously and the run list refreshes immediately.</li>
  <li><strong>GitHub</strong> — a <code>workflow_dispatch</code> event is fired (HTTP 204). GitHub queues the run asynchronously, so the list refreshes automatically after a 3-second delay.</li>
</ul>

<h2>What you can do</h2>
<table class="shortcuts-table">
  <thead><tr><th>Action</th><th>How</th></tr></thead>
  <tbody>
    <tr><td>View recent runs</td><td>Switch to the <strong>CI / CD</strong> tab — the last 30 runs are fetched automatically</td></tr>
    <tr><td>Create a new run</td><td>Click the <strong>Run</strong> button in the CI header — opens branch/variable picker</td></tr>
    <tr><td>Refresh the list</td><td>Click the <RefreshCw size={11} /> button in the panel header</td></tr>
    <tr><td>View stage/job graph</td><td>Click any run card to open the detail modal</td></tr>
    <tr><td>Re-trigger a run</td><td>Click <RotateCcw size={11} /> in the run card or inside the detail modal</td></tr>
    <tr><td>Open run in browser</td><td>Click <ExternalLink size={11} /> in the run card or modal header</td></tr>
    <tr><td>Open a specific job's logs</td><td>Click a job card inside the detail modal</td></tr>
  </tbody>
</table>

<h2>Run status mapping</h2>
<table class="shortcuts-table">
  <thead><tr><th>Arbor status</th><th>GitHub</th><th>GitLab</th></tr></thead>
  <tbody>
    <tr><td>✅ Passed</td><td><code>completed / success</code></td><td><code>success</code>, <code>passed</code></td></tr>
    <tr><td>❌ Failed</td><td><code>completed / failure</code>, <code>timed_out</code></td><td><code>failed</code></td></tr>
    <tr><td>⏳ Running</td><td><code>in_progress</code>, <code>queued</code></td><td><code>running</code></td></tr>
    <tr><td>⭕ Cancelled</td><td><code>completed / cancelled</code>, <code>skipped</code></td><td><code>canceled</code>, <code>skipped</code></td></tr>
    <tr><td>🔵 Pending</td><td><code>waiting</code>, <code>requested</code></td><td><code>pending</code>, <code>created</code>, <code>scheduled</code></td></tr>
  </tbody>
</table>

<h2>Self-hosted GitLab</h2>
<p>Self-hosted GitLab instances are auto-detected from the remote URL (any host containing <code>gitlab.</code>). Store a personal access token via <strong>Settings → Additional Git Credentials</strong> using the instance hostname as the key. Arbor will use that token for all API calls to that host.</p>
