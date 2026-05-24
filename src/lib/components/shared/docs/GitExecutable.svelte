<script lang="ts">
  import Callout from '$lib/components/shared/ui/Callout.svelte';
</script>

<h1>Git Executable</h1>

<p class="doc-lead">
  Arbor uses libgit2 (via the <code>git2</code> crate) for most operations, but a handful of commands —
  rebase, stash, submodule update, recovery snapshots, fast-forward / non-FF merges — still shell out to
  the system <code>git</code> binary. This page covers how Arbor finds that binary, how to override it,
  and how to install one when you don't have it.
</p>

<h2>Detection order</h2>

<p>At startup (and again whenever you click <strong>Re-detect</strong> in Settings), Arbor resolves the path
in this order:</p>

<ol class="prop-list">
  <li><strong>Override path</strong>The <code>executable_path</code> set under <code>[git]</code> in
    <code>~/.config/arbor/config.toml</code>. Set via <strong>Settings → Git → Git Executable → Browse</strong>.</li>
  <li><strong>System <code>PATH</code></strong>The first <code>git</code> (or <code>git.exe</code> on Windows)
    found by walking the directories in your <code>PATH</code> environment variable.</li>
  <li><strong>Bundled portable copy</strong><code>~/.config/arbor/git/cmd/git.exe</code> on Windows
    (populated by the in-app downloader). Skipped on macOS / Linux.</li>
</ol>

<p>The <strong>Settings → Git → Git Executable</strong> page shows which of the three is currently active
via the <em>source</em> pill (<code>config</code>, <code>path</code>, or <code>portable</code>).</p>

<h2>First-launch flow</h2>

<p>If detection turns up nothing on first launch, Arbor opens a blocking <strong>Git Setup</strong> modal
that you can't dismiss until the path is resolved. Three actions:</p>

<ul class="prop-list">
  <li><strong>Download portable git (Windows only)</strong>Grabs the latest PortableGit from
    <code>git-for-windows/git</code> on GitHub and unpacks it into Arbor's config folder. Around 50 MB; progress
    streams into the modal.</li>
  <li><strong>Browse for git executable…</strong>Pick the <code>git</code> binary you want to use. Arbor
    runs <code>--version</code> against it before saving — bad paths are rejected.</li>
  <li><strong>Auto-detect</strong>Re-scan PATH and the bundled copy. Useful when you installed git
    while Arbor was already open.</li>
</ul>

<Callout variant="info" title="Why a blocking modal?">
  Without git, anything that depends on the CLI (rebase, stash apply, submodule update) silently fails.
  Forcing the user to resolve the path up-front prevents confusing partial-functionality states.
</Callout>

<h2>Installing git on macOS / Linux</h2>

<p>The in-app download is Windows-only because there's no clean cross-distro portable build of git for
Unix. Use your package manager:</p>

<table class="shortcuts-table">
  <thead><tr><th>Platform</th><th>Command</th></tr></thead>
  <tbody>
    <tr><td>macOS</td><td><code>brew install git</code></td></tr>
    <tr><td>Debian / Ubuntu</td><td><code>sudo apt install git</code></td></tr>
    <tr><td>Fedora / RHEL</td><td><code>sudo dnf install git</code></td></tr>
    <tr><td>Arch</td><td><code>sudo pacman -S git</code></td></tr>
  </tbody>
</table>

<p>Then click <strong>Auto-detect</strong> in the modal (or in <strong>Settings → Git → Git Executable</strong>).</p>

<h2>Switching between several installs</h2>

<p>If you keep multiple git versions around (e.g. system git for daily use, a custom build for testing
a patch), use <strong>Browse</strong> to pin Arbor to a specific path. The <em>Clear override</em> button
falls back to PATH / portable lookup without touching the on-disk binary.</p>

<h2>Downloaded portable copy</h2>

<p>On Windows, the <strong>Download portable</strong> button writes to
<code>%APPDATA%\arbor\git\</code>. The directory contains a full PortableGit tree
(<code>cmd/</code>, <code>bin/</code>, <code>etc/</code>, …). To remove it, delete the folder — Arbor
will fall back to PATH on next launch.</p>

<Callout variant="info" title="Updating the portable copy">
  Re-running <strong>Download portable</strong> from Settings overwrites the existing extraction with
  the latest release. The active path is repointed automatically.
</Callout>

<h2>Authentication</h2>

<p>
  When Arbor shells out to git for clone, ls-remote, submodule fetch/pull/push,
  or the post-fetch step of an MR conflict resolution, it injects the OAuth
  token (or PAT) you saved under <strong>Settings → Authentication</strong> as a
  host-scoped HTTP header:
</p>

<pre><code>{`git -c http.https://github.com/.extraHeader="Authorization: Bearer …" \\
    -c http.https://github.com/.helper= \\
    clone https://github.com/owner/repo.git`}</code></pre>

<p>
  The <code>helper=</code> override clears the OS-level credential chain
  <em>only for that host</em> so a partially-stored Git Credential Manager
  entry can't conflict with Arbor's token.  Hosts Arbor doesn't have a token
  for fall back to the normal git behaviour: SSH keys via
  <code>~/.ssh</code> / ssh-agent for <code>git@host:</code> URLs, and GCM /
  netrc / system helper for HTTPS URLs.
</p>

<p>
  In practice this means:
</p>

<ul class="prop-list">
  <li><strong>Authenticated via Arbor</strong>Just works.  Clone, submodule fetch, and conflict-resolution fetch all use your saved Arbor credentials.</li>
  <li><strong>Authenticated only via SSH</strong>Use <code>git@host:owner/repo</code> URLs.  Arbor doesn't touch those — they go straight to ssh-agent.</li>
  <li><strong>Authenticated only via GCM / netrc</strong>Continue to work for any host Arbor doesn't have a token for.  When Arbor does have a token, it wins for that host — refresh or remove it from Settings → Authentication if you'd rather defer to the OS.</li>
</ul>

<Callout variant="info" title="libgit2 vs CLI.">
  Network operations done through libgit2 (the main repo's <em>fetch</em>
  and <em>push</em>) have always used Arbor's stored credentials.  This
  page is about the CLI shell-outs, which historically deferred to the OS
  helper — they now align with libgit2's behaviour.
</Callout>

<h2>Plugins should not shell out to <code>git</code></h2>

<Callout variant="warning" title="For plugin authors.">
  <code>arbor.terminal.exec("git ...")</code> uses the system <code>PATH</code>, NOT the binary
  configured here.  That means a plugin that shells out to <code>git</code> directly will silently
  bypass the user's choice — it can run a different version, miss the bundled portable copy
  entirely, or fail on machines where Arbor's PortableGit is the only git available.
  Use the built-in APIs instead (<code>arbor.repo.fetch_active_tab</code>, <code>arbor.repo.clone</code>,
  …).  If the operation you need isn't exposed, file an issue rather than working around it
  with a raw shell call — Arbor doesn't auto-rewrite plugin commands by design, since that
  would change their semantics behind the author's back.
</Callout>

<h2>Config file</h2>

<p>The override is stored in <code>~/.config/arbor/config.toml</code>:</p>

<pre><code>{`[git]
executable_path = "C:/Tools/Git/cmd/git.exe"`}</code></pre>

<p>Setting <code>executable_path</code> to an empty string or removing the key falls back to the
detection chain.</p>
