<script lang="ts">
  import Callout from '$lib/components/shared/ui/Callout.svelte';
</script>

<h1>Deep Links (<code>arbor://</code>)</h1>

<p class="doc-lead">
  Arbor registers the <code>arbor://</code> URI scheme on your OS so links shared by colleagues,
  CI bots, browser extensions, or desktop shortcuts can drop you straight into the right place
  inside Arbor — no copy-pasting branch names or commit SHAs.  Arbor brings the existing window
  to the foreground (single-instance), or starts cold if it isn't running yet.
</p>

<Callout variant="warning" title="Off by default.">
  Deep links are disabled out of the box and every action kind is individually opt-in.  An
  incoming URL on a fresh install is intercepted and shown as a "Deep Link Blocked" modal.
  Turn the master switch on in <strong>Settings → Tools → Deep Links → Master switch</strong>,
  then enable each action you want to accept under <strong>Enabled actions</strong>.
</Callout>

<h2>URL shape</h2>

<p>
  Every URL identifies the <strong>repository</strong> with a <code>?url=</code> query parameter
  carrying the <em>remote git URL</em> (HTTPS or SSH).  Local paths would be useless across
  machines, so they're never used as deep-link identifiers.  Arbor looks the URL up against your
  registered repositories using a fuzzy host/owner/repo key — <code>https://github.com/foo/bar.git</code>,
  <code>git@github.com:foo/bar</code>, <code>ssh://git@github.com/Foo/Bar.git/</code> all match the same clone.
</p>

<table class="shortcuts-table">
  <thead><tr><th>URL</th><th>Action</th></tr></thead>
  <tbody>
    <tr>
      <td><code>arbor://repo/open?url=&lt;url&gt;</code></td>
      <td>Open the repository (or clone it)</td>
    </tr>
    <tr>
      <td><code>arbor://commit/&lt;sha&gt;?url=&lt;url&gt;</code></td>
      <td>Switch to the repo and jump to a commit in the graph</td>
    </tr>
    <tr>
      <td><code>arbor://branch/&lt;name&gt;?url=&lt;url&gt;&amp;checkout=1</code></td>
      <td>Stash-safe checkout of the named branch</td>
    </tr>
    <tr>
      <td><code>arbor://branch/&lt;name&gt;?url=&lt;url&gt;&amp;worktree=1</code></td>
      <td>Open the "Add worktree" dialog pre-filled with the branch</td>
    </tr>
    <tr>
      <td><code>arbor://mr/open/&lt;number&gt;?url=&lt;url&gt;</code></td>
      <td>Open the merge / pull request detail modal</td>
    </tr>
    <tr>
      <td><code>arbor://pipeline/&lt;run-id&gt;?url=&lt;url&gt;</code></td>
      <td>Open the CI pipeline run detail modal</td>
    </tr>
  </tbody>
</table>

<h2>Generating links from inside Arbor</h2>

<p>Every action above has a <strong>"Copy arbor:// link"</strong> entry point in the UI:</p>

<ul class="prop-list">
  <li><strong>Repository</strong>The link icon next to the commit count in the graph toolbar copies <code>arbor://repo/open</code>.</li>
  <li><strong>Branch</strong>Right-click any local or remote branch in the sidebar → "Copy arbor:// checkout link".</li>
  <li><strong>Worktree</strong>Right-click any worktree row in the sidebar (or use the link icon in the worktree info modal) → "Copy arbor:// worktree link".</li>
  <li><strong>Merge / pull request</strong>Right-click any row in the MR sidebar, or use the link button in the MR detail modal header.</li>
  <li><strong>CI pipeline run</strong>Right-click any row in the Pipelines panel, or use the link button in the CI run detail modal header.</li>
  <li><strong>Commit</strong>Right-click any node in the graph, or use the link button in the commit detail panel.</li>
</ul>

<p>
  All of these embed the active repository's <strong>origin remote URL</strong> in the
  <code>?url=</code> parameter (falling back to the first remote when no <code>origin</code>
  exists).  When the repo has no remotes configured, the copy buttons stay enabled but show a
  warning toast — there's no shareable URL to embed.
</p>

<h2>The three gates</h2>

<p>Every incoming <code>arbor://</code> URL passes through three gates before anything happens:</p>

<ol class="prop-list">
  <li><strong>Master enable</strong>If off (default), the dispatcher short-circuits to a "Deep Link Blocked" modal that names the feature and points the user at Settings.  Nothing else runs.</li>
  <li><strong>Per-action enable</strong>Even with the master on, each action kind (<em>open repo</em>, <em>jump to commit</em>, <em>checkout branch</em>, <em>create worktree</em>, <em>open MR</em>, <em>open pipeline</em>) has its own toggle, all default off.  A blocked action shows the same disabled modal but names the specific kind.</li>
  <li><strong>Per-action confirm</strong>If both gates above let the URL through, the dispatcher shows the action-confirm modal explaining what will happen.  Confirms can be disabled per-action for trusted flows (e.g. read-only commit jumps).</li>
</ol>

<h2>Confirmation prompts</h2>

<p>
  Every deep-link action shows an <strong>"Are you sure?"</strong> modal by default
  before doing anything — Arbor never executes a shared link without an explicit click.
  The prompt names the action ("Check out branch <code>feature/x</code>") and shows
  the target git URL so you can sanity-check who's asking.
</p>

<p>
  In <strong>Settings → Tools → Deep Links → Confirmations</strong> you can disable the prompt
  per-action for the cases you trust (e.g. <em>commit jump</em>, which is read-only).
  The clone-confirm dialog is independent of these toggles: if the local copy is missing,
  Arbor always asks before cloning — it has to, you need to pick the destination folder.
</p>

<h2>Routing rules</h2>

<p>The dispatcher resolves the target repo using the registry, then applies these rules:</p>

<ul class="prop-list">
  <li><strong>In the active workspace</strong>Activate the existing tab (open one if the repo is registered but not currently a tab).</li>
  <li><strong>In another workspace</strong>Apply your <strong>Cross-workspace target</strong> setting — either switch workspace and activate the tab, or surface it as a cross-workspace tab in the workspace you're already in.</li>
  <li><strong>Registered but not in any workspace</strong>Add it to the active workspace and open the tab.</li>
  <li><strong>Not in the registry</strong>Show the clone-confirm dialog (folder picker + Clone & Continue button).  If you cancel, nothing happens.</li>
  <li><strong>Local copy missing on disk</strong>Same clone-confirm dialog, but the wording explains the local copy is gone.  Re-cloning replaces the missing folder transparently and the action proceeds.</li>
</ul>

<h2>Checkout links → worktrees</h2>

<p>
  The <strong>Checkout links create a worktree</strong> setting silently rewrites incoming
  <code>arbor://branch/&lt;name&gt;?checkout=1</code> URLs to the worktree variant before
  dispatch.  Useful when your workflow is "every shared branch becomes its own worktree" —
  the shared link never moves HEAD on your main checkout.  The Add Worktree dialog opens
  pre-filled with the branch, you pick the destination, nothing happens to disk until you
  click <strong>Add</strong>.  Links you copy out of Arbor still embed the literal action
  they were built from — the rewrite only applies to incoming links you receive.
</p>

<h2>Cross-workspace strategy</h2>

<p>Configurable in <strong>Settings → Tools → Deep Links</strong>:</p>

<ul class="prop-list">
  <li><strong>Switch to that workspace</strong> (default)Changes the active workspace to the first one that owns the target repo (in the order shown in your workspace dropdown), then activates the tab inside it.</li>
  <li><strong>Open here as cross-workspace tab</strong>Adds the repo to the workspace you're currently focused on, marked with the cross-workspace dot.  Doesn't disturb your current focus.</li>
</ul>

<h2>Cold start vs. warm path</h2>

<p>
  Arbor uses single-instance mode for deep links: clicking <code>arbor://…</code> while Arbor is
  already running brings the existing window to the foreground and forwards the URL to it.
  Clicking it while Arbor is closed launches the app and processes the URL after the UI has had a
  chance to mount — URLs received in the boot window are buffered, then drained the moment the
  frontend is ready.  In both cases you don't see a blank window or a missed action.
</p>

<h2>Dev-mode toggle</h2>

<p>
  Deep-link support is always on in release builds.  In debug builds it's gated behind the
  <code>deep-link-dev</code> Cargo feature, which is currently in <code>default</code> so it works
  out of the box.  Drop it from the default features (and rerun with
  <code>--features deep-link-dev</code>) to test how the app behaves without it.
</p>
