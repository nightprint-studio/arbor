<script lang="ts">
  import { highlight } from '$lib/utils/diff-formatter';
</script>

<h1>Initialize Repository</h1>

<p class="doc-lead">When you open a folder that isn't a git repository, Arbor detects this automatically and offers to initialize one â€” no need to run <code>git init</code> in a terminal.</p>

<h2>The flow</h2>
<ol class="step-list">
  <li>Either open the hamburger menu and pick <strong>Initialize Repositoryâ€¦</strong>, or press <kbd>Ctrl+O</kbd> and select any folder without a <code>.git</code> directory</li>
  <li>The <strong>Initialize Repository</strong> dialog opens automatically</li>
  <li>Configure options across three tabs: <strong>Project</strong>, <strong>Files</strong>, <strong>Remote</strong></li>
  <li>Click <strong>Initialize Repository</strong> or press <kbd>Ctrl+Enter</kbd></li>
  <li>The repo is created and opens as a new tab immediately</li>
</ol>
<p>The menu entry routes you straight into the dialog regardless of whether the folder already has a <code>.git</code> directory: a folder that's already a repo just opens normally, no destructive re-init.</p>

<h2>Project tab</h2>
<ul>
  <li><strong>Description</strong> â€” stored in <code>.git/description</code> and added to the README if enabled</li>
  <li><strong>Default branch</strong> â€” choose <code>main</code>, <code>master</code>, <code>develop</code>, or a custom name</li>
  <li><strong>Initial commit</strong> â€” stages all created files and makes the first commit automatically</li>
  <li><strong>Author name / email</strong> â€” pre-filled from the global git config (<code>user.name</code> / <code>user.email</code>)</li>
</ul>

<h2>Files tab</h2>
<div class="feature-grid">
  <div class="feature-card">
    <div class="fc-title">README.md</div>
    <div class="fc-desc">Generated with the repo name as H1 and the description as body text.</div>
  </div>
  <div class="feature-card">
    <div class="fc-title">.gitignore</div>
    <div class="fc-desc">Pick from built-in templates: Rust, Node/JS/TS, Python, Go, Java, C, C++, .NET/C#, Swift, Ruby, PHP, Unity.</div>
  </div>
  <div class="feature-card">
    <div class="fc-title">LICENSE</div>
    <div class="fc-desc">MIT, Apache 2.0, GPL 3, LGPL 3, AGPL 3, BSD 2/3-Clause, ISC, MPL 2.0 â€” filled with your name and the current year.</div>
  </div>
</div>

<h2>Remote tab</h2>
<p>Optionally create and link a remote repository at init time:</p>
<table class="shortcuts-table">
  <thead><tr><th>Option</th><th>What happens</th></tr></thead>
  <tbody>
    <tr><td><strong>None</strong></td><td>Local repo only â€” add a remote later</td></tr>
    <tr><td><strong>GitHub</strong></td><td>Creates the repo via the GitHub API and adds it as <code>origin</code>. Requires a GitHub token in Settings â†’ Credentials.</td></tr>
    <tr><td><strong>GitLab</strong></td><td>Creates the project via the GitLab API. Supports organizations/groups. Requires a GitLab token.</td></tr>
    <tr><td><strong>Custom URL</strong></td><td>Adds any URL as <code>origin</code> without an API call (Gitea, Forgejo, self-hosted instances).</td></tr>
  </tbody>
</table>

<div class="callout tip">
  <strong>API failure is non-fatal</strong>
  If the provider API call fails, the local repository is still initialized. Arbor shows an error toast but the repo opens normally.
</div>

<h2>Plugin hook: <code>on_repo_init</code></h2>
<p>Fires after a repository is successfully initialized and opened. Declare in <code>plugin.toml</code>:</p>
<pre class="language-toml">{@html highlight(`[hooks]
on_repo_init = true`, 'toml')}</pre>

<p>Register a handler in Lua:</p>
<pre class="language-lua">{@html highlight(`arbor.events.on("on_repo_init", function(ctx)
  -- ctx.path           -- absolute path to the repo
  -- ctx.name           -- folder name
  -- ctx.default_branch -- e.g. "main"
  -- ctx.provider       -- "none" | "github" | "gitlab" | "custom"
  -- ctx.remote_url     -- "" or the configured remote origin URL
  -- ctx.has_readme     -- bool
  -- ctx.license        -- "" or SPDX id (e.g. "mit")
  -- ctx.gitignore      -- "" or template name (e.g. "rust")
  arbor.notify{ title = "Repository initialized", message = ctx.name .. " created on " .. ctx.default_branch, level = "success" }
end)`, '.lua')}</pre>
