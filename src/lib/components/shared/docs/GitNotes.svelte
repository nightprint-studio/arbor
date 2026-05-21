<div class="doc-section">
  <h2>Git Notes</h2>
  <p>
    Git notes let you attach freeform text to any commit <em>without modifying the commit itself</em>.
    Notes are stored in a parallel ref (<code>refs/notes/&lt;namespace&gt;</code>) so the commit hash is
    never changed — useful for personal annotations, code-review remarks, or linking external context.
  </p>

  <h3>Key Concepts</h3>
  <ul>
    <li><strong>Namespace</strong> — each note belongs to a namespace (e.g. <code>commits</code>, <code>review</code>, <code>jira</code>). The default git namespace is <code>commits</code>. Namespaces follow git ref naming rules: no spaces, no <code>~^:?*[\</code>, no <code>..</code>, and cannot start or end with <code>.</code>.</li>
    <li><strong>Local vs Remote</strong> — notes are <em>not</em> pushed automatically with <code>git push</code>. You must push them explicitly with <code>git push origin refs/notes/commits</code>. Arbor shows the remote sync status of each note.</li>
    <li><strong>Compatibility</strong> — notes are plain text; any git client can read them with <code>git log --show-notes</code>.</li>
  </ul>

  <h3>Using Notes in Arbor</h3>
  <h4>Adding a Note</h4>
  <p>Right-click any commit in the graph and choose <strong>Notes…</strong>, or click the <strong>Notes</strong> row in the Commit Detail panel.</p>
  <p>In the modal, click <strong>Add note</strong> and fill in:</p>
  <ul>
    <li><strong>Namespace</strong> — defaults to <code>commits</code>. Use a different name to separate concerns (e.g. <code>review</code>, <code>deploy</code>).</li>
    <li><strong>Content</strong> — freeform text.</li>
  </ul>

  <h4>Graph Badge</h4>
  <p>Commits that have at least one note show a small pill (with a count) right after the commit message in the graph. Clicking it opens the notes modal directly.</p>

  <h4>Remote Status</h4>
  <p>When the modal opens, Arbor checks each note against its remote tracking ref (<code>refs/remotes/origin/notes/&lt;namespace&gt;</code>):</p>
  <ul>
    <li><strong>Local only</strong> — note exists only locally; never pushed.</li>
    <li><strong>In sync</strong> — local and remote blobs match.</li>
    <li><strong>Out of sync</strong> — local note differs from remote (local is ahead).</li>
  </ul>
  <p>Use the <strong>refresh</strong> icon on each note to re-check its remote status after a push.</p>

  <h3>Plugin API — <code>arbor.notes</code></h3>
  <p>Requires <code>git = "read"</code> for read operations, <code>git = "write"</code> for write operations.</p>

  <table>
    <thead><tr><th>Function</th><th>Description</th></tr></thead>
    <tbody>
      <tr>
        <td><code>arbor.notes.list(commit_oid)</code></td>
        <td>Returns an array of <code>&#123; namespace, content, created_at, remote_status &#125;</code> for the active tab's commit. <code>created_at</code> is a Unix timestamp (seconds).</td>
      </tr>
      <tr>
        <td><code>arbor.notes.get(commit_oid, namespace)</code></td>
        <td>Returns the note content string, or <code>nil</code> if no note exists.</td>
      </tr>
      <tr>
        <td><code>arbor.notes.set&#123; commit_oid, namespace, content &#125;</code></td>
        <td>Create or overwrite a note. Returns <code>(true, nil)</code> on success, <code>(false, err)</code> on git failure. Fires <code>on_note_saved</code> hook.</td>
      </tr>
      <tr>
        <td><code>arbor.notes.delete(commit_oid, namespace)</code></td>
        <td>Delete a note. Fires <code>on_note_deleted</code> hook.</td>
      </tr>
    </tbody>
  </table>

  <h4>Example</h4>
  <pre><code>-- Auto-annotate commits that reference a Jira ticket
arbor.events.on("on_commit", function(ctx)
  local msg = ctx.summary or ""
  local ticket = msg:match("[A-Z]+%-%d+")
  if ticket then
    arbor.notes.set&#123; commit_oid = ctx.oid, namespace = "jira", content = ticket &#125;
  end
end)</code></pre>

  <h3>Plugin Hooks</h3>
  <table>
    <thead><tr><th>Hook</th><th>Context fields</th></tr></thead>
    <tbody>
      <tr>
        <td><code>on_note_saved</code></td>
        <td><code>tab_id</code>, <code>commit_oid</code>, <code>namespace</code></td>
      </tr>
      <tr>
        <td><code>on_note_deleted</code></td>
        <td><code>tab_id</code>, <code>commit_oid</code>, <code>namespace</code></td>
      </tr>
    </tbody>
  </table>

  <h3>Plugin Manifest</h3>
  <pre><code>[hooks]
on_note_saved   = true
on_note_deleted = true

[permissions]
git = "write"</code></pre>
</div>

<style>
  .doc-section { font-family: var(--font-ui-sans); color: var(--text-secondary); font-size: var(--font-size-sm); line-height: 1.65; }
  h2 { font-size: 1.15em; font-weight: 700; color: var(--text-primary); margin: 0 0 10px; }
  h3 { font-size: 0.9em; font-weight: 600; color: var(--text-primary); margin: 18px 0 6px; text-transform: uppercase; letter-spacing: 0.4px; }
  h4 { font-size: 0.85em; font-weight: 600; color: var(--text-primary); margin: 12px 0 4px; }
  p  { margin: 0 0 8px; }
  ul { margin: 0 0 8px; padding-left: 18px; }
  li { margin-bottom: 4px; }
  code { font-family: var(--font-code); font-size: 11px; background: var(--bg-overlay); padding: 1px 4px; border-radius: var(--radius-sm); color: var(--accent); }
  pre { background: var(--bg-overlay); border: 1px solid var(--border-subtle); border-radius: var(--radius-md); padding: 10px 12px; overflow-x: auto; margin: 8px 0 12px; }
  pre code { background: none; padding: 0; color: var(--text-secondary); }
  table { width: 100%; border-collapse: collapse; font-size: 12px; margin: 8px 0 12px; }
  th { text-align: left; padding: 5px 8px; background: var(--bg-overlay); color: var(--text-primary); font-weight: 600; border-bottom: 1px solid var(--border); }
  td { padding: 5px 8px; border-bottom: 1px solid var(--border-subtle); vertical-align: top; }
</style>
