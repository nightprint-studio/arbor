<script lang="ts">
  import { highlight } from '$lib/utils/diff-formatter';
</script>

<h1>Issues â€” Linear &amp; Jira</h1>

<p class="doc-lead">Browse, filter, and act on issues directly from the sidebar without switching context. Each repository can independently use either tracker.</p>

<div class="feature-grid two-col">
  <div class="feature-card accent">
    <div class="fc-eyebrow">Linear</div>
    <div class="fc-title">OAuth Â· Personal API Key</div>
    <div class="fc-desc">Full read/write access. Attach issues to branches, transition statuses, post comments from plugins.</div>
  </div>
  <div class="feature-card accent">
    <div class="fc-eyebrow">Jira</div>
    <div class="fc-title">Cloud Â· Data Center Â· Server</div>
    <div class="fc-desc">Email + API token, PAT for DC/Server, OAuth 2.0 (3LO) for Cloud. Self-signed certs accepted.</div>
  </div>
</div>

<h2>Setup</h2>
<p>Open the <strong>Issues</strong> sidebar and pick a tracker â€” or configure credentials in <strong>Settings â†’ Git &amp; Integrations â†’ Issue Trackers</strong>. Each repository stores its own selection.</p>

<div class="divider">Linear</div>

<h3>OAuth <span class="badge badge-new">Recommended</span></h3>
<ol class="step-list">
  <li>Register a <strong>Public OAuth application</strong> at <code>linear.app â†’ Settings â†’ API â†’ OAuth applications</code></li>
  <li>Add <code>http://127.0.0.1:7729/callback</code> as the redirect URI</li>
  <li>Click <strong>Connect â†’ OAuth</strong> in settings and approve in the browser</li>
  <li>Arbor completes the PKCE flow and stores the token in the OS keychain</li>
</ol>

<h3>Personal API Key</h3>
<ol class="step-list">
  <li>Generate a key at <code>linear.app â†’ Settings â†’ API â†’ Personal API keys</code></li>
  <li>Click <strong>Connect â–¾ â†’ Personal API Key</strong> and paste the <code>lin_api_â€¦</code> token</li>
</ol>

<div class="divider">Jira</div>

<h3>API Token â€” Jira Cloud <span class="badge badge-new">Recommended</span></h3>
<p>Generate an API token at <code>id.atlassian.com â†’ Security â†’ API tokens</code>, then click <strong>Connect â†’ API Token</strong> and fill in:</p>
<dl class="meta-grid">
  <dt>Subdomain</dt><dd>The part before <code>.atlassian.net</code> (e.g. <code>mycompany</code>)</dd>
  <dt>Email</dt><dd>Your Atlassian account email</dd>
  <dt>API token</dt><dd>The token just generated</dd>
</dl>

<h3>Personal Access Token â€” Data Center / Server</h3>
<p>Generate a PAT at <code>Jira â†’ Profile â†’ Personal Access Tokens</code>. Use the <strong>API Token</strong> form with the full hostname as the subdomain (e.g. <code>jira.internal.example.com</code>) plus email and PAT.</p>
<div class="hint">Arbor automatically accepts self-signed or internal-CA certificates common in on-premise Jira installations.</div>

<h3>OAuth 2.0 (3LO) â€” Jira Cloud only</h3>
<p>Click <strong>Connect â–¾ â†’ OAuth 2.0</strong> and follow the browser prompt. Arbor auto-discovers your site and stores access + refresh tokens in the OS keychain. Token refresh is transparent.</p>

<h3>Jira compatibility matrix</h3>
<table class="matrix">
  <thead><tr><th>Edition</th><th>Auth</th><th>API</th><th>Notes</th></tr></thead>
  <tbody>
    <tr><td>Cloud <code>*.atlassian.net</code></td><td>Token Â· OAuth 2.0</td><td>v3</td><td>Full feature set</td></tr>
    <tr><td>Data Center â‰¥ 8.4</td><td>Email + PAT</td><td>v2</td><td>Self-signed certs OK</td></tr>
    <tr><td>Server / DC &lt; 8.4</td><td>Email + PAT</td><td>v2</td><td>Uses <code>/project</code> endpoint</td></tr>
  </tbody>
</table>

<h2>Sidebar</h2>
<p>Same UI for both providers. Filters combine freely.</p>
<ul class="prop-list">
  <li>
    <strong>Search</strong>
    Debounced 350 ms. Two modes:
    <dl class="meta-grid">
      <dt><code>PROJ-42</code></dt>
      <dd><strong>Default</strong> â€” matches the ticket code <em>and</em> any text that mentions it. Free-form text (e.g. <code>login bug</code>) falls back to text matching across title / description / comments.</dd>
      <dt><code>~PROJ-42</code></dt>
      <dd><strong>Text-only</strong> â€” the <code>~</code> prefix bypasses the code lookup. Finds only descriptions / comments / titles that <em>mention</em> <code>PROJ-42</code>, never the ticket whose key is <code>PROJ-42</code>. Useful for tracing references without the noise of the ticket card on top.</dd>
    </dl>
    <div class="hint">
      On Jira the text side searches <code>summary + description + comments</code> (the JQL <code>text ~</code> operator). On Linear it searches <code>title</code> only â€” the GraphQL filter doesn't expose body / comment search.
    </div>
  </li>
  <li><strong>Me</strong>Show only issues assigned to you.</li>
  <li><strong>Status</strong>Multi-select grouped by type: backlog / unstarted / started / completed / cancelled. Falls back to statuses derived from loaded issues when the API returns none.</li>
  <li><strong>Team / Project</strong>Linear team or Jira project. Search box appears when more than 5 options exist. Jira fetches all paginated pages alphabetically.</li>
  <li><strong>Issue Type</strong>Jira only. Multi-select by type (Bug, Story, Task, Epic, Sub-taskâ€¦) with per-type colour indicators.</li>
  <li><strong>Milestone</strong>Linear project milestone or Jira fix version.</li>
  <li><strong>Sprint / Cycle</strong>Jira active sprint or Linear cycle.</li>
  <li><strong>+</strong>Open the Create Issue form.</li>
</ul>

<h3>Issue card</h3>
<p>Priority emoji Â· Identifier (<code>ARB-123</code>) Â· Title Â· Labels Â· Status badge Â· Assignee avatar Â· Time-ago Â· Comment count. <strong>Click</strong> opens the detail modal, <strong>right-click</strong> the context menu.</p>

<h3>Ticket picker</h3>
<p>Appears when creating a branch via GitFlow or the graph context menu. Uses the active repo's tracker automatically â€” no need to open the sidebar first. Same filters as the sidebar; selecting an issue populates the branch name.</p>

<h2>Detail modal</h2>
<p>Click an issue card to open the full detail view: metadata sidebar, description, attachments, linked commits, and threaded comments.</p>

<h3>Description &amp; comments rendering</h3>
<p>Bodies are rendered with full styling â€” headings, lists, code blocks, tables, blockquotes, panels, mentions, status lozenges:</p>
<ul class="prop-list">
  <li><strong>Linear</strong>Markdown rendered in-app via the shared sanitised renderer (same used by PR/MR bodies). Inline HTML safelist supports collapsible <code>&lt;details&gt;</code> / <code>&lt;summary&gt;</code>, tables, blockquotes and code; fenced blocks without an explicit language are auto-detected (Rust, TOML, JSON, YAML, bash, TS/JS, markup) and highlighted with Prism.</li>
  <li><strong>Jira</strong>Server-rendered HTML via <code>expand=renderedFields</code> (covers ADF on Cloud and wiki markup on Server / Data Center). HTML is sanitized with <code>ammonia</code> before display â€” scripts, iframes, event handlers and inline styles are stripped; <code>class</code> survives so syntax highlighting and panel chrome land correctly.</li>
</ul>

<h3>Attachments</h3>
<p>Jira issues with attached files show a grid of cards between the description and the linked commits. Each card has a type-aware icon (image / video / audio / pdf / archive / text / generic), filename, size, and MIME type.</p>
<ul class="prop-list">
  <li><strong>Click to download</strong>Opens Arbor's in-app save picker with the original filename pre-filled. The fetch only starts after you confirm a destination â€” cancelling the picker is a true no-op.</li>
  <li><strong>Authenticated &amp; streamed</strong>The download runs on the Tokio runtime off the UI thread, and the body is streamed chunk-by-chunk straight to disk â€” no whole-file buffering in RAM, no UI freeze.</li>
  <li><strong>Host-locked</strong>The backend rejects download URLs whose host doesn't match the configured Jira instance, so the IPC command can't be coerced into acting as a generic authenticated proxy.</li>
  <li><strong>Status feedback</strong>The card icon becomes a spinner while downloading and a green âœ“ on success; failures show a red border and a toast.</li>
</ul>

<h2>Jira field mapping</h2>
<table>
  <thead><tr><th>Arbor concept</th><th>Jira field</th><th>Notes</th></tr></thead>
  <tbody>
    <tr><td>Teams</td><td>Projects</td><td>Project key used for JQL (<code>project = "KEY"</code>)</td></tr>
    <tr><td>Issue Type</td><td>Issue Type</td><td>Bug / Story / Task / Epic / Sub-task; colour per type</td></tr>
    <tr><td>Status</td><td>Status</td><td>Status category â†’ type (unstarted / started / completed)</td></tr>
    <tr><td>Labels</td><td>Labels</td><td>Plain strings; colour deterministic</td></tr>
    <tr><td>Priority</td><td>Priority</td><td>Highest â†’ Urgent, High, Medium, Low/Lowest</td></tr>
    <tr><td>Cycle</td><td>Sprint</td><td>Active sprints via Agile API (Jira Software only)</td></tr>
    <tr><td>Milestone</td><td>Fix Version</td><td>First fix version on the issue</td></tr>
    <tr><td>Estimate</td><td>Story Points</td><td><code>customfield_10016</code></td></tr>
  </tbody>
</table>

<h2>Create Issue</h2>
<p>Two-column form â€” title/description left, metadata right.</p>
<div class="feature-grid two-col">
  <div class="feature-card">
    <div class="fc-title">Linear fields</div>
    <div class="fc-desc">Team Â· Status Â· Priority Â· Project Â· Milestone Â· Assignee (self) Â· Labels Â· Due date Â· Estimate.</div>
  </div>
  <div class="feature-card">
    <div class="fc-title">Jira fields</div>
    <div class="fc-desc">Project <span class="badge badge-req">Req</span> Â· Issue Type <em>(default: Task)</em> Â· Priority Â· Labels Â· Assignee (self) Â· Fix Version Â· Due date Â· Story Points.</div>
  </div>
</div>

<h2>Plugin API â€” <code>arbor.issues</code></h2>
<p>Works identically for Linear and Jira â€” the active provider for each repo is resolved transparently.</p>
<dl class="meta-grid">
  <dt><code>issues = "read"</code></dt><dd>Enables <code>arbor.issues.search()</code> and <code>arbor.issues.get()</code></dd>
  <dt><code>issues = "write"</code></dt><dd>Enables <code>arbor.issues.transition()</code> and <code>arbor.issues.comment()</code> <span class="badge badge-accent">implies read</span></dd>
</dl>
<pre class="language-lua"><code>{@html highlight(`local issues = arbor.issues.search({
  query        = "login",
  assigneeMe   = true,
  statusIds    = { "10001", "10002" },    -- Jira status IDs or Linear workflow-state UUIDs
  labelIds     = { "bug" },               -- Jira: label name; Linear: label UUID
  issueTypeIds = { "Bug", "Story" },      -- Jira only (ignored on Linear)
  teamId       = "PROJ",                  -- Jira: project key; Linear: team UUID
  limit        = 25,
})

for _, issue in ipairs(issues) do
  print(issue.identifier, issue.title, issue.status.name)
end

-- Transition issue (Jira resolves status ID â†’ workflow transition automatically)
arbor.issues.transition(issue.id, status_id)

-- Add a comment
arbor.issues.comment(issue.id, "Deployed to staging âœ“")

-- Branch name slug
local branch = arbor.issues.branch_name(issue)
-- Linear: "arb-123-fix-login-bug"
-- Jira:   "proj-456-fix-login-bug"`, '.lua')}</code></pre>

<h2>Plugin hooks</h2>
<table>
  <thead><tr><th>Constant</th><th>Event</th><th>Context fields</th></tr></thead>
  <tbody>
    <tr>
      <td><code>hooks.ISSUE_LINKED</code></td>
      <td><code>on_issue_linked</code></td>
      <td><code>issue_id</code>, <code>identifier</code>, <code>sha</code>, <code>branch</code></td>
    </tr>
    <tr>
      <td><code>hooks.ISSUE_TRANSITIONED</code></td>
      <td><code>on_issue_transitioned</code></td>
      <td><code>issue_id</code>, <code>identifier</code>, <code>from_status</code>, <code>to_status</code></td>
    </tr>
  </tbody>
</table>
