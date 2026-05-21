<h1>Settings — Access</h1>

<p>
  The <strong>Git &amp; Integrations</strong> section consolidates Git host accounts, credentials, and issue tracker connections into a single place.
  All secrets are stored in the OS keychain (Windows Credential Manager, macOS Keychain, libsecret on Linux).
</p>

<h2>Git Providers (GitHub / GitLab)</h2>
<p>
  Each provider card has a split <strong>Connect</strong> button. Click the main button to connect via OAuth,
  or click the <strong>▾</strong> chevron to pick a different method:
</p>
<ul>
  <li>
    <strong>GitHub OAuth — Device Authorization Grant (RFC 8628)</strong>.
    Arbor calls GitHub to obtain a <em>user code</em>, opens
    <code>https://github.com/login/device</code> in your default browser, and shows the code in the panel.
    Copy or click the open-page button, paste the code on github.com, and approve.
    Arbor polls the token endpoint in the background and stores the access token in the OS keychain.
    No callback server, no client secret — the flow uses only the public <code>client_id</code>.
  </li>
  <li>
    <strong>GitLab OAuth — Authorization Code + PKCE</strong>.
    Arbor starts a one-shot local callback server on <code>127.0.0.1:7731</code>, opens the GitLab
    authorization page, exchanges the returned code for a token, and stores it in the OS keychain.
  </li>
  <li><strong>Personal Access Token</strong> — paste a PAT directly. Stored in the keychain and used for HTTPS operations.</li>
  <li><strong>Username + Password</strong> — store a username and password/token pair. Used automatically for fetch, pull, and push.</li>
</ul>
<p>For self-hosted GitLab, check <strong>Self-hosted</strong> and enter your instance hostname before saving — or use the <strong>Advanced</strong> panel below to point the OAuth flow at your private GitLab instance.</p>

<h3>Connected-user badge</h3>
<p>
  Once a GitHub or GitLab connection settles into <em>connected</em>, the
  card replaces the raw <code>client_id</code> blob with a compact
  <strong>user badge</strong>: avatar, display name, and a secondary line
  (email or <code>@login</code>). Each line is click-to-copy — a tick
  flashes in place of the icon to confirm the copy. The badge is rendered
  by the shared <code>ProviderUserBadge</code> widget, so Linear / Jira /
  Atlassian connections (when the provider exposes <code>/me</code>) all
  get the same treatment with no per-card boilerplate.
</p>
<p>
  Data comes from new <code>get_github_user</code> / <code>get_gitlab_user</code>
  IPCs that call the provider's <code>current_user()</code> on connect; if
  the call fails (revoked token, offline) the badge silently falls back to
  the connection summary — no error toast for what is purely cosmetic.
</p>

<h2>Advanced — use your own OAuth application</h2>
<p>
  Each OAuth provider card has an <strong>Advanced — use my own OAuth app</strong> toggle that expands an
  override panel.  Use it when:
</p>
<ul>
  <li>You forked Arbor and want OAuth tokens issued under your own GitHub / GitLab / Linear / Atlassian app.</li>
  <li>You're behind a corporate proxy that requires a captive client.</li>
  <li>You're connecting to a <strong>self-hosted GitLab</strong> instance that issues its own OAuth applications (set both <code>client_id</code> and <code>base_host</code>, e.g. <code>gitlab.company.com</code>).</li>
</ul>
<p>
  Overrides are persisted in plain TOML at <code>~/.config/arbor/config.toml</code> under <code>[oauth.&lt;provider&gt;]</code>.
  The OAuth <code>client_id</code> is a public identifier (RFC 6749 §2.2) and is intentionally not stored in the keychain — only access and refresh tokens are.
  Leave a field empty to fall back to Arbor's bundled default.
</p>
<p>
  Redirect / callback hints when registering your own app:
</p>
<ul>
  <li><strong>GitHub</strong> — Device Flow only.  Enable <em>Device Flow</em> in your OAuth App settings.  No callback URL needed.</li>
  <li><strong>GitLab</strong> — Redirect URI: <code>http://127.0.0.1:7731/callback</code>, scope <code>api</code>, <em>Confidential</em> off (PKCE replaces the secret).</li>
  <li><strong>Linear</strong> — Redirect URI: <code>http://127.0.0.1:7729/callback</code>, public client.</li>
  <li><strong>Jira / Atlassian</strong> — Redirect URI: <code>http://127.0.0.1:7730/callback</code>, scopes <code>read:jira-work write:jira-work offline_access read:me</code>.</li>
</ul>
<p>
  Changing the <code>client_id</code> invalidates any refresh token obtained with the previous one — you'll be re-prompted to authorise the new app on the next refresh attempt.
</p>

<h2>Additional Git Credentials</h2>
<p>
  The <strong>Additional Git Credentials</strong> card lets you store credentials for other hosts
  (Bitbucket, Azure DevOps, custom Git servers). Select a provider preset or choose <em>Custom…</em> and enter the host manually.
</p>

<h2>Issue Trackers (Linear / Jira)</h2>
<p>Each tracker card uses the same split <strong>Connect</strong> button pattern — click the main button for the default method or <strong>▾</strong> for alternatives.</p>

<h3>Linear</h3>
<ul>
  <li>
    <strong>OAuth (recommended)</strong> — Authorization Code + PKCE with a localhost callback server on port 7729.
    Arbor ships with a bundled OAuth app — just click <strong>Authorize</strong> and Arbor opens the browser and completes the flow automatically.
    To use your own OAuth app instead, expand <strong>Advanced — use my own OAuth app</strong> and set the <code>client_id</code> (register a <em>Public</em> app at
    <code>linear.app → Settings → API → OAuth applications</code> with <code>http://127.0.0.1:7729/callback</code> as redirect URI).
  </li>
  <li>
    <strong>Personal API Key</strong> — generate at <code>linear.app → Settings → API → Personal API keys</code> and paste directly.
  </li>
</ul>

<h3>Jira</h3>
<ul>
  <li>
    <strong>API Token — Jira Cloud</strong> — generate at <code>id.atlassian.com → Security → API tokens</code>.
    Enter your subdomain (the part before <code>.atlassian.net</code>), email, and the token.
  </li>
  <li>
    <strong>Personal Access Token — Jira Data Center / Server</strong> — generate at <code>Jira → Profile → Personal Access Tokens</code>.
    Enter the full hostname as subdomain (e.g. <code>jira.internal.example.com</code>), your email, and the PAT.
    Self-signed and internal-CA certificates are accepted automatically.
  </li>
  <li>
    <strong>OAuth 2.0 (3LO) — Jira Cloud only</strong> — click <strong>Connect ▾ → OAuth 2.0</strong> and follow the browser prompt.
    Arbor discovers your Cloud site automatically and stores access + refresh tokens in the OS keychain.
    To use your own Atlassian OAuth 2.0 (3LO) app, expand <strong>Advanced — use my own Atlassian OAuth app</strong> on the Jira card and set the <code>client_id</code> (register at
    <code>developer.atlassian.com → OAuth 2.0 (3LO)</code> with <code>http://127.0.0.1:7730/callback</code> as redirect URI).
  </li>
</ul>
<p>See the <strong>Issues (Linear / Jira)</strong> section for the full compatibility table, sidebar filters, and plugin API.</p>
