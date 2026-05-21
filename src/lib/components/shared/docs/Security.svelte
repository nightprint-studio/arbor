<script lang="ts">
  import { highlight } from '$lib/utils/diff-formatter';
</script>

<h1>Security Dashboard</h1>

<p class="doc-lead">
  GitLab- and GitHub-native security posture inside Arbor: severity counters,
  risk score, vulnerabilities-over-time chart, and a virtualized findings
  modal â€” gated automatically per repo so it shows up only where the provider
  has data.
</p>

<div class="feature-grid two-col">
  <div class="feature-card accent">
    <div class="fc-eyebrow">GitLab</div>
    <div class="fc-title">Vulnerability Report</div>
    <div class="fc-desc">GraphQL: severity counts Â· time series Â· risk score Â· per-finding metadata. Ultimate-only fields degrade gracefully.</div>
  </div>
  <div class="feature-card accent">
    <div class="fc-eyebrow">GitHub</div>
    <div class="fc-title">GHAS Â· Dependabot Â· Secret Scanning</div>
    <div class="fc-desc">Three REST sources merged into one finding list. Time series unavailable (GitHub doesn't expose it).</div>
  </div>
</div>

<h2>Authentication &amp; visibility</h2>
<p>
  No extra setup â€” the same OAuth token used by the MR/CI panels is reused.
  When the active tab's repo has a remote on a supported host, Arbor fires
  a lightweight provider probe (<code>supports_security</code>); the
  Activity Bar icon and the StatusBar chip become live as soon as it
  resolves. Tokenless repos and providers that don't expose the dashboard
  see a clear "not available" state instead of a broken icon.
</p>

<h2>Activity Bar entry</h2>
<p>
  Click the <strong>ShieldAlert</strong> icon in the left Activity Bar
  (top group, after Branches). The icon is always rendered; the panel
  itself decides what to show:
</p>
<ul class="prop-list">
  <li><strong>Probing</strong><span>Spinner + "Checking providerâ€¦" while the support probe is in flight.</span></li>
  <li><strong>Not available</strong><span>Static copy explaining likely causes (no GitHub/GitLab remote, missing token, plan without scanning), with a Re-check button.</span></li>
  <li><strong>Loading summary</strong><span>Standard spinner for the headline fetch.</span></li>
  <li><strong>Loaded</strong><span>Filter bar Â· 6 severity counter cards Â· risk-score gauge + vulns-over-time chart Â· truncation note when the cap is hit.</span></li>
</ul>

<h2>Headline counters</h2>
<p>
  Six cards: <strong>Critical / High / Medium / Low / Info / Unknown</strong>.
  Each shows the <em>active</em> count and the median age in the band's
  colour (e.g. <em>9 mo</em>, <em>113 days</em>). Clicking a non-zero card
  opens the detail modal at that severity tab.
</p>

<h3>What "active" means</h3>
<p>
  The dashboard <strong>always excludes resolved and dismissed findings</strong>.
  Both backends enforce this: GitLab passes <code>state: [DETECTED, CONFIRMED]</code>
  to <code>vulnerabilitySeveritiesCount</code>; GitHub already filters via
  <code>?state=open</code>. Closed findings live behind the modal's scope toggle.
</p>

<h2>Risk score &amp; time series</h2>
<p>
  The risk gauge renders a 0â€“100 score with bands (Low / Medium / High /
  Critical). The score is a host-side heuristic
  <code>(criticalÃ—10 + highÃ—5 + mediumÃ—2 + lowÃ—0.5)</code> capped at 100.
</p>
<p>
  The vulnerabilities-over-time chart pulls 30/60/90 day windows.
  GitLab Ultimate exposes <code>vulnerabilitiesCountByDay</code>; GitHub
  doesn't, so the chart is hidden on GitHub repos.
</p>
<p class="hint">
  When the panel is narrow, the gauge and chart automatically stack
  vertically â€” the layout uses a CSS container query, so it tracks the
  panel width rather than the viewport.
</p>

<h2>Detail modal</h2>
<p>
  Opened from a counter card click. Layout:
</p>
<ul class="prop-list">
  <li><strong>Header</strong><span>Shield icon Â· risk pill Â· "Open in &lt;provider&gt;" external link.</span></li>
  <li><strong>Tabs</strong><span>Per-severity strip (<code>All | Critical | High | â€¦</code>) â€” counts dynamic, zero tabs disabled.</span></li>
  <li><strong>Scope toggle</strong><span>Two-button segmented control beside the tabs: <code>Active</code> (default) shows Detected + Confirmed, <code>Closed</code> shows Resolved + Dismissed. Persisted in <code>localStorage</code>.</span></li>
  <li><strong>Progress bar</strong><span>Indeterminate sliding bar at the top of the list region â€” shows during fetches AND during tab/scope swaps to mask the DOM-thrash on large severities.</span></li>
  <li><strong>Virtualized list</strong><span>Each row is a fixed 64px so the list can render 300+ findings as ~20 DOM nodes. Severity desc â†’ age desc sort.</span></li>
  <li><strong>Footer</strong><span>"<em>Showing N of M findings</em>" plus a truncation hint when the host-side cap kicked in.</span></li>
</ul>

<h3>Finding-detail modal</h3>
<p>
  Click any row in the list to open a dedicated <strong>per-finding</strong>
  modal that lifts the full payload above the aggregate view. Layout:
</p>
<ul class="prop-list">
  <li><strong>Header</strong><span>Severity chip Â· title Â· CVE / report-type chips Â· "Open in &lt;provider&gt;".</span></li>
  <li><strong>Remediation</strong>
    <span>
      Prominent <em>"How to fix"</em> block when the provider exposes one.
      GitLab â†’ <code>Vulnerability.solution</code> as-is. GitHub Dependabot â†’
      synthetic hint built from <code>first_patched_version</code>:
      <em>"Upgrade `pkg` to `X` or later (vulnerable range: `R`)"</em>.
      Markdown-rendered, so links and code-fences in vendor advisories render
      correctly.
    </span>
  </li>
  <li><strong>Metadata grid</strong><span>Identifiers (CVE, CWE), file/line, package + version, age, last-detected, state history.</span></li>
  <li><strong>Description &amp; references</strong><span>Long-form Markdown body + outbound links to advisories/CVE pages.</span></li>
</ul>

<h3>Active vs Closed scope</h3>
<p>
  The toggle <em>only</em> affects the modal â€” the dashboard's counter
  grid, gauge, chart, and the StatusBar chip always stay on the active
  scope. Switching to <code>Closed</code> refetches with
  <code>state: [RESOLVED, DISMISSED]</code> and lets you audit the
  finding hygiene without polluting the headline numbers.
</p>

<h2>Filter bar</h2>
<p>
  Above the counter grid:
</p>
<ul class="prop-list">
  <li><strong>Search</strong><span>Host-side substring match on title + file path. 250 ms debounce.</span></li>
  <li><strong>Severity multiselect</strong><span>Narrows counters, chart, and the modal list.</span></li>
  <li><strong>Type multiselect</strong><span>Auto-populates from the loaded findings â€” <code>sast</code>, <code>dependency_scanning</code>, <code>secret_detection</code>, â€¦ etc.</span></li>
  <li><strong>Clear</strong><span>Resets severity / type / search but <em>preserves the state scope</em> â€” the user's scope choice is treated as a view mode, not a narrowing filter.</span></li>
</ul>

<h2>StatusBar chip</h2>
<p>
  Left side of the footer, right after the branch chip. Shield icon with
  a corner badge showing the total active finding count
  (<code>99+</code> when capped). Tooltip carries the per-severity
  breakdown. Click â†’ floating Quick Overlay anchored to the left of the
  footer with the full severity rundown plus
  <em>Open dashboard</em> / <em>Open in provider</em> shortcuts.
</p>

<h2>Caching</h2>
<p>The store dedupes concurrent IPC calls and persists the user-facing knobs:</p>
<ul class="prop-list">
  <li><strong>Probe cache</strong><span>Per-tab support result kept in memory for the session. Refreshing the panel invalidates it.</span></li>
  <li><strong>In-flight dedup</strong><span><code>loadSummary</code> / <code>loadFindings</code> share a single Promise per tab â€” the AppShell pre-load and the panel mount fetch can fire concurrently without racing each other into a stuck loading state.</span></li>
  <li><strong>localStorage</strong><span>Range (30/60/90), severity filter, report-type filter, state scope.</span></li>
</ul>

<h2>Lua API</h2>
<p>
  Plugins read posture data via <code>arbor.security.*</code>. The token
  never leaves the host â€” provider permission gate is the same
  <code>provider = "read"</code> flag used by <code>arbor.mr.*</code> and
  <code>arbor.ci.*</code>.
</p>

<pre class="language-lua"><code>{@html highlight(`-- Cheap probe â€” false for tokenless repos / providers without a dashboard
local ok, err = arbor.security.supports({ repo_id = "myrepo" })

-- Headline summary (active findings only). Same shape the panel renders.
local summary, err = arbor.security.summary({
  repo_id    = "myrepo",
  range_days = 90,            -- optional, clamped to [7, 90], default 30
})
-- summary.counts        : { critical, high, medium, low, info, unknown }
-- summary.median_age_days
-- summary.risk_score    : { value: number, label: string } | nil
-- summary.time_series   : { points = {...}, range_days } | nil
-- summary.web_url       : provider-native dashboard URL

-- Findings list. Defaults to active scope; opt into closed by passing states.
local list, err = arbor.security.findings({
  repo_id      = "myrepo",
  severities   = {"critical", "high"},   -- optional
  states       = {"resolved", "dismissed"}, -- optional, default {detected,confirmed}
  report_types = {"sast", "secret_detection"},
  search       = "deserialization",
  limit        = 200,
})
for _, f in ipairs(list) do
  -- f.solution is non-nil on GitLab and on GitHub Dependabot (synthetic
  -- "Upgrade ... to X" hint from first_patched_version). On code-scanning
  -- and secret-scanning it stays nil.
  arbor.log.info("[%s] %s â€” %s%s",
                 f.severity, f.title, f.web_url or "no url",
                 f.solution and (" Â· fix: " .. f.solution) or "")
end`, 'lua')}</code></pre>

<h2>Hooks</h2>
<p>Two hooks contribute to the <code>security</code> category:</p>
<dl class="meta-grid">
  <dt><code>on_security_summary_loaded</code></dt>
  <dd>
    Fired by the host after every successful summary fetch. Payload:
    <code>&lbrace; tab_id, provider, counts, total, risk_label?, web_url? &rbrace;</code>.
    All <code>counts</code> values are active-only. Use it for
    notifications when posture worsens, or to mirror counts to an external
    dashboard.
  </dd>
  <dt><code>on_security_finding_state_changed</code></dt>
  <dd>
    A plugin-cooperation channel: when a plugin observes a finding moving
    between active and closed states (e.g. a periodic rescan), it can
    emit this hook so other plugins can react. The host itself does NOT
    emit it on every fetch â€” keeps the channel signal-only.
    Payload: <code>&lbrace; tab_id, finding_id, severity, from_state?, to_state, title?, web_url? &rbrace;</code>.
  </dd>
</dl>

<h3>Example: notify on new Critical findings</h3>
<pre class="language-lua"><code>{@html highlight(`-- plugins/security-watch/main.lua
local last_critical = {}   -- repo_id â†’ previous critical count

arbor.events.on("on_security_summary_loaded", function(ctx)
  local prev = last_critical[ctx.tab_id] or 0
  local now  = (ctx.counts and ctx.counts.critical) or 0
  if now > prev then
    arbor.notify({
      title   = "New critical vulnerabilities",
      message = string.format("%s: %d new (was %d) â€” open the dashboard.",
                              ctx.tab_id, now - prev, prev),
      level   = "warning",
    })
  end
  last_critical[ctx.tab_id] = now
end)`, 'lua')}</code></pre>

<h2>Permissions &amp; manifest</h2>
<p>
  All <code>arbor.security.*</code> calls require the <code>provider</code>
  permission at <code>read</code> level (or higher) in
  <code>plugin.toml</code>:
</p>
<pre class="language-toml"><code>{@html highlight(`# plugin.toml
[permissions]
provider = "read"   # read-only access to MR/CI/security`, 'toml')}</code></pre>

<h2>Provider differences (cheat sheet)</h2>
<table class="shortcuts-table">
  <thead><tr><th>Capability</th><th>GitLab</th><th>GitHub</th></tr></thead>
  <tbody>
    <tr><td>Dashboard probe</td><td>GraphQL (<code>vulnerabilitySeveritiesCount</code> + <code>vulnerabilities</code>)</td><td>REST x3 (code-scanning Â· dependabot Â· secret-scanning)</td></tr>
    <tr><td>Severity counts</td><td>Server-side, state-filtered</td><td>Computed host-side from open alerts</td></tr>
    <tr><td>Time series</td><td>Ultimate-only via <code>vulnerabilitiesCountByDay</code></td><td>Not exposed â†’ chart hidden</td></tr>
    <tr><td>Risk score</td><td>Heuristic (host-side)</td><td>Heuristic (host-side)</td></tr>
    <tr><td>Self-hosted</td><td>Host-keyed PAT in keychain</td><td>n/a</td></tr>
  </tbody>
</table>
