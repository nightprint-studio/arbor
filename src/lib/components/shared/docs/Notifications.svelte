<h1>Notifications</h1>

<p class="doc-lead">The notification center collects in-app alerts from plugins. Notifications persist until explicitly dismissed — so you never miss a build result or error.</p>

<h2>Bell badge (status bar)</h2>
<p>A bell icon in the status bar (right side) shows the current notification count. Click it to open the <strong>Notifications overlay</strong>.</p>

<h2>Notification overlay</h2>
<p>A floating panel anchored above the status bar. Each notification shows:</p>
<ul>
  <li>A coloured left border and icon matching its level (info / success / warning / error)</li>
  <li><strong>Title</strong> and <strong>message</strong> from the plugin that fired it</li>
  <li>Source plugin name badge</li>
  <li>Relative timestamp (<em>"2s ago"</em>, <em>"5m ago"</em>…)</li>
  <li>An optional <strong>action button</strong> if the notification was emitted with one — clicking it runs the associated side-effect (e.g. opens the Linked Worktrees manager) and dismisses the notification</li>
  <li><strong>×</strong> to dismiss the individual notification</li>
</ul>
<p>The <strong>trash icon</strong> in the header clears all notifications at once.</p>

<h2>Notification actions</h2>
<p>Plugins can attach a click action to a notification.  Built-in action kinds:</p>
<table class="shortcuts-table">
  <thead><tr><th>Kind</th><th>Required fields</th><th>Effect</th></tr></thead>
  <tbody>
    <tr><td><code>open-link-manager</code></td><td><code>label</code>, <code>link_id</code></td><td>Opens the Linked Worktrees manager pre-selected on that link.</td></tr>
    <tr><td><code>open-tab-by-repo-id</code></td><td><code>label</code>, <code>repo_id</code></td><td>Activates the matching open tab; no-op if not currently open.</td></tr>
    <tr><td><code>open-url</code></td><td><code>label</code>, <code>url</code></td><td>Opens the URL in the user's default <strong>browser</strong>. Use <code>open-path</code> instead for local files (<code>file://</code> URLs are silently ignored by the opener plugin).</td></tr>
    <tr><td><code>open-path</code></td><td><code>label</code>, <code>path</code>, <code>reveal?</code></td><td>Hands the path to the OS' default handler (folder → Explorer/Finder, file → default editor). Set <code>reveal = true</code> to open the file's parent folder instead — the cross-platform "reveal in Explorer".</td></tr>
    <tr><td><code>plugin-action</code></td><td><code>label</code>, <code>plugin</code>, <code>action</code>, <code>ctx?</code></td><td>Fires <code>arbor.events.on(action, …)</code> in the named plugin with the optional <code>ctx</code> table — round-trip back to a plugin handler from the click.</td></tr>
  </tbody>
</table>

<h2>Plugin API</h2>
<p>Plugins emit notifications through the table-config form of <code>arbor.notify</code>:</p>
<pre><code>arbor.notify{`{`}
  message = "Build failed (exit 1) — see log",   -- required
  title   = "compile-action",                   -- optional
  level   = "error",                            -- "info"|"success"|"warning"|"error"
  action  = {`{`} kind = "open-link-manager", label = "View link", link_id = "abc" {`}`},
{`}`}
</code></pre>
<p>See the <strong>Plugin Development</strong> section for the full API reference.</p>
