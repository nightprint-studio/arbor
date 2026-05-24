<script lang="ts">
  import { highlight } from '$lib/utils/diff-formatter';
</script>

<div class="doc">
  <h1>Ticket Links</h1>
  <p>
    Arbor can associate commits with tickets from your issue tracker — automatically
    by parsing commit messages and branch names, or manually via right-click.
    Linked tickets appear as small chips on each graph row and in the commit detail panel.
  </p>

  <h2>How it works</h2>
  <ul>
    <li><strong>Auto-detect (message)</strong> — Arbor scans each visible commit message for
      ticket IDs matching the configured tracker pattern (e.g. <code>ENG-123</code> for Linear,
      <code>#456</code> for GitHub / GitLab). Results are cached in memory — no re-scan on scroll.</li>
    <li><strong>Auto-detect (branch)</strong> — Branch names pointing to a commit are also scanned
      (e.g. <code>feature/ENG-123-login-flow</code>).</li>
    <li><strong>Manual link</strong> — Right-click a commit → <em>Link to ticket…</em> to open the
      ticket picker and create a persistent association stored in the backing store.</li>
  </ul>

  <h2>Storage backends</h2>
  <p>
    Manual links can be stored in one of two backends. The backend is exclusive —
    only one is active per repository at a time (no mixed reads).
  </p>

  <table>
    <thead>
      <tr><th>Backend</th><th>Location</th><th>Distributed on push?</th></tr>
    </thead>
    <tbody>
      <tr>
        <td><code>git_notes</code> <em>(default)</em></td>
        <td><code>refs/notes/arbor/tickets</code> in the git object store</td>
        <td>Only if you configure the push refspec (see below)</td>
      </tr>
      <tr>
        <td><code>links_toml</code></td>
        <td><code>.arbor/links.toml</code> in the repository root</td>
        <td>Yes, if you commit and push the file</td>
      </tr>
    </tbody>
  </table>

  <h2>Configuration</h2>
  <p>
    Global defaults live in <code>~/.config/arbor/config.toml</code>.
    Per-repository overrides go in <code>.arbor/config.toml</code> inside the repo.
    Project settings take precedence.
  </p>

  <h3>Global config (<code>~/.config/arbor/config.toml</code>)</h3>
  <pre class="language-toml">{@html highlight(`[ticket_links]
enabled    = true          # master switch (also in Settings → Graph)
storage    = "git_notes"   # "git_notes" | "links_toml"
auto_parse = true          # parse commit messages + branch names
warn_push  = true          # warn when notes push refspec is missing`, 'toml')}</pre>

  <h3>Per-repo config (<code>.arbor/config.toml</code>)</h3>
  <pre class="language-toml">{@html highlight(`[ticket_links]
storage        = "links_toml"      # override the global backend for this repo
tracker        = "linear"          # "linear" | "jira" | "github" | "gitlab"
auto_parse     = true
custom_pattern = "\\\\b(MYCO-\\\\d+)\\\\b"  # optional — overrides the tracker default`, 'toml')}</pre>
  <p>
    <code>custom_pattern</code> can also be set via <strong>Settings → Repository → Ticket Links</strong>
    without editing the TOML file manually. The value must be a valid Rust regex with exactly
    one capture group — the captured text becomes the ticket ID.
  </p>

  <p>
    <strong>Tip:</strong> <code>tracker</code> can also be set via the existing
    <code>issue_tracker</code> field in <code>.arbor/config.toml</code> — the
    ticket-links system inherits it as a fallback.
  </p>

  <h2>Sharing git notes with teammates</h2>
  <p>
    By default, <code>git push</code> does not include notes.
    Add the following to your <code>.git/config</code> (or run the equivalent
    <code>git config</code> commands) to push and fetch notes automatically:
  </p>
  <pre><code>[remote "origin"]
    fetch = +refs/notes/*:refs/notes/*
    push  = refs/notes/*</code></pre>
  <p>
    Arbor will warn you after a push if this refspec is not yet configured.
  </p>

  <h2>UI elements</h2>
  <ul>
    <li><strong>Graph chips</strong> — Colored pill badges on each row.
      Color indicates the tracker: purple = Linear / Jira, grey = GitHub, orange = GitLab.
      Click to open the issue detail. Hover a manually-added chip to reveal the ✕ remove button.</li>
    <li><strong>Commit detail panel</strong> — "Tickets" row below the commit body
      showing all linked tickets. Manual links have an ✕ button to remove them.</li>
    <li><strong>Right-click → Link to ticket…</strong> — Opens the ticket picker
      to create a manual association.</li>
    <li><strong>Issue detail → Linked Commits</strong> — When viewing a ticket in the
      issues sidebar, a <em>Linked Commits</em> section loads lazily and shows every
      commit associated with that ticket (both auto-detected and manual). Each entry
      displays the short SHA, summary, author, date, and branch chips (when the
      commit is already in the graph cache). Click any entry to navigate directly to
      that commit in the graph.</li>
    <li><strong>Settings → Graph → Ticket link chips</strong> — Toggle to disable the
      feature entirely if you experience scroll slowdowns on very large repos.</li>
  </ul>

  <h2>Reverse lookup: ticket → commits</h2>
  <p>
    The <em>Linked Commits</em> section in the issue detail provides full reverse lookup:
  </p>
  <ul>
    <li><strong>Manual links (git notes)</strong> — All notes under
      <code>refs/notes/arbor/tickets</code> are scanned.</li>
    <li><strong>Manual links (links.toml)</strong> — The full
      <code>.arbor/links.toml</code> file is read (served from cache when warm).</li>
    <li><strong>Auto-detected</strong> — Commits already scrolled into view whose
      message or branch name matched the ticket ID are included. Commits not yet
      loaded in the graph are not covered by auto-detection (scroll more of the
      graph to widen the search).</li>
  </ul>

  <h2>Ticket ID patterns</h2>
  <table>
    <thead><tr><th>Tracker</th><th>Default pattern</th><th>Example</th></tr></thead>
    <tbody>
      <tr><td>Linear</td><td><code>[A-Z][A-Z0-9]*-\d+</code></td><td><code>ENG-123</code>, <code>PROJ-42</code></td></tr>
      <tr><td>Jira</td><td><code>[A-Z][A-Z0-9]*-\d+</code></td><td><code>PROJ-456</code>, <code>ABC-7</code></td></tr>
      <tr><td>GitHub</td><td><code>#\d+</code></td><td><code>#456</code>, <code>fixes #789</code></td></tr>
      <tr><td>GitLab</td><td><code>#\d+</code></td><td><code>#123</code></td></tr>
    </tbody>
  </table>
  <p>
    Any tracker's default pattern can be overridden with a <strong>custom regex</strong> per repository.
    Set it in <strong>Settings → Repository → Ticket Links</strong> or directly in
    <code>.arbor/config.toml</code>:
  </p>
  <pre class="language-toml">{@html highlight(`[ticket_links]
tracker        = "jira"
custom_pattern = "\\\\b(MYCO-\\\\d+)\\\\b"   # must have exactly one capture group`, 'toml')}</pre>
  <p>
    When <code>custom_pattern</code> is set it takes full precedence — the tracker default is ignored.
    The captured text (group 1) becomes the ticket ID stored and displayed on the chip.
    Invalid regex is silently ignored and the tracker default is used instead.
  </p>
</div>

<style>
  .doc { font-family: var(--font-ui-sans); font-size: 13px; color: var(--text-primary); line-height: 1.6; }
  h1 { font-size: 18px; font-weight: 700; margin: 0 0 12px; color: var(--text-primary); }
  h2 { font-size: 14px; font-weight: 600; margin: 20px 0 8px; color: var(--text-primary); }
  h3 { font-size: 12px; font-weight: 600; margin: 14px 0 6px; color: var(--text-secondary); }
  p  { margin: 0 0 10px; color: var(--text-secondary); }
  ul { margin: 0 0 10px; padding-left: 18px; color: var(--text-secondary); }
  li { margin-bottom: 4px; }
  strong { color: var(--text-primary); font-weight: 600; }
  code {
    font-family: var(--font-code);
    font-size: 11px;
    background: var(--bg-hover);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-sm);
    padding: 1px 4px;
    color: var(--accent);
  }
  pre {
    background: var(--bg-overlay, var(--bg-elevated));
    border: 1px solid var(--border);
    border-radius: var(--radius-md);
    padding: 10px 12px;
    overflow-x: auto;
    margin: 8px 0 12px;
  }
  pre code {
    background: none;
    border: none;
    padding: 0;
    font-size: 11px;
    color: var(--text-secondary);
    white-space: pre;
  }
  table {
    width: 100%;
    border-collapse: collapse;
    font-size: 12px;
    margin: 8px 0 12px;
  }
  th, td {
    padding: 6px 10px;
    text-align: left;
    border-bottom: 1px solid var(--border-subtle);
    color: var(--text-secondary);
  }
  th { font-weight: 600; color: var(--text-primary); background: var(--bg-elevated); }
  em { font-style: italic; color: var(--text-muted); }
</style>
