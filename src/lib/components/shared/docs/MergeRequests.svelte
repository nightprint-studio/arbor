<script lang="ts">
  import { highlight } from '$lib/utils/diff-formatter';
</script>

<h1>Pull / Merge Requests</h1>

<p class="doc-lead">Browse, review, and merge GitHub Pull Requests and GitLab Merge Requests from the sidebar. Reuses the same OAuth tokens as the CI/CD panel — no separate setup.</p>

<div class="feature-grid two-col">
  <div class="feature-card accent">
    <div class="fc-eyebrow">GitHub</div>
    <div class="fc-title">Pull Requests</div>
    <div class="fc-desc">Merge · Squash · Rebase · CI checks panel · Reopen.</div>
  </div>
  <div class="feature-card accent">
    <div class="fc-eyebrow">GitLab</div>
    <div class="fc-title">Merge Requests</div>
    <div class="fc-desc">Default strategy · Self-hosted instances supported · Reopen.</div>
  </div>
</div>

<h2>Authentication</h2>
<p>Connect your accounts in <strong>Settings → Git &amp; Integrations</strong>. The same tokens used for CI/CD are reused — no extra setup. Click the <strong>GitPullRequest</strong> icon in the Activity Bar to open the sidebar.</p>

<h2>Sidebar</h2>
<ul class="prop-list">
  <li><strong>Search bar</strong>Client-side fuzzy filter over the loaded list — matches title, <code>#number</code>, source/target branches, author display name &amp; login, and label names. Clear with the <strong>×</strong> button. The query resets on tab switch.</li>
  <li><strong>Filter tabs</strong>Switch between <em>Open</em> and <em>Merged</em> PRs/MRs. Backend reload — the search bar then narrows whichever set is loaded.</li>
  <li><strong>Row content</strong>Status icon · title · number · source → target · author · time-ago · comment count · labels.</li>
  <li><strong>Click row</strong>Opens the detail modal.</li>
  <li><strong>Header +</strong>Create a new PR/MR.</li>
  <li><strong>Refresh</strong>Reload the list from the API.</li>
</ul>

<h2>Detail modal</h2>
<p>Four tabs across the top: <strong>Overview</strong>, <strong>CI</strong>, <strong>Files</strong>, <strong>Commits</strong>. Press <kbd>Esc</kbd> to close. The header has a <strong>refresh</strong> button that reloads detail + list + every tab that's already been opened.</p>
<p>The minimize button in the header parks a "reopen" shortcut in the status-bar dock — useful when you need to check a commit or branch and come back to the MR later. The status-bar badge (next to notifications) shows how many dialogs are parked; click it to see the list and pick which to restore. Restoring switches back to the source tab and re-opens the dialog, so the chip survives tab and workspace changes; scroll position and unsubmitted comments do not — the modal re-fetches and re-renders.</p>
<div class="feature-grid">
  <div class="feature-card">
    <div class="fc-title">Header</div>
    <div class="fc-desc">State badge (Open / Merged / Closed) · Draft flag · title · branches · author · time-ago · labels · refresh · open in browser.</div>
  </div>
  <div class="feature-card">
    <div class="fc-title">Overview</div>
    <div class="fc-desc">Markdown description · CI Checks summary (when available) · Assignees · Reviewers · Activity timeline.</div>
  </div>
</div>

<h4>Markdown rendering</h4>
<p>PR/MR bodies, descriptions, and comments share a single sanitised renderer. Dependabot, ReleaseDrafter, and other bots that pack large amounts of structure into the body render correctly now:</p>
<ul class="prop-list">
  <li><strong>Inline HTML safelist</strong><code>&lt;details&gt;</code> / <code>&lt;summary&gt;</code> (collapsible cards with a chevron), <code>&lt;p&gt;</code>, <code>&lt;blockquote&gt;</code>, <code>&lt;code&gt;</code>, <code>&lt;ul&gt;</code> / <code>&lt;ol&gt;</code>, <code>&lt;table&gt;</code> and friends survive verbatim. Scripts, styles, iframes, event handlers, and raw <code>&lt;a&gt;</code> tags are stripped or rewritten.</li>
  <li><strong>Fence language auto-detect</strong>fenced blocks without an explicit language (<code>```</code> without a tag) are sniffed (<em>Rust, TOML, JSON, YAML, bash, TS/JS, markup</em>) and highlighted deterministically — no more wall-of-grey for bot-generated diffs.</li>
  <li><strong>Markdown also applies to inline contexts</strong>same renderer is wired into the Issues detail modal so Linear / Jira (when ADF returns markdown) get the same treatment.</li>
</ul>
<div class="feature-grid">
  <div class="feature-card">
    <div class="fc-title">CI</div>
    <div class="fc-desc">Pipeline runs targeting the source branch — status pill, duration, retrigger, click to open the stage/job graph.</div>
  </div>
  <div class="feature-card">
    <div class="fc-title">Files / Commits</div>
    <div class="fc-desc">Per-file diff view and commit-by-commit drill-down with syntax highlighting.</div>
  </div>
  <div class="feature-card">
    <div class="fc-title">Actions</div>
    <div class="fc-desc">Merge (split button) · Reopen (merged) · Close (with confirmation dialog).</div>
  </div>
</div>

<h3>Activity timeline</h3>
<p>The Overview tab renders comments and timeline events on a GitLab-style vertical rail. Three filter chips at the top toggle each category — counts always reflect what's loaded, regardless of visibility:</p>
<dl class="meta-grid">
  <dt><span class="chip-doc chip-doc-blue">Comments</span></dt>
  <dd>Human-authored comments — large avatar nodes, accent-blue strip on the left edge of each card. Body rendered as Markdown (headings, lists, fenced code blocks with Prism syntax highlighting, blockquotes, links).</dd>
  <dt><span class="chip-doc chip-doc-yellow">Bots <em>2</em></span></dt>
  <dd>
    Comments from automated accounts. Heuristic: GitHub login ending with <code>[bot]</code> or <code>github-actions</code>; GitLab login/name containing "bot". Bot cards get a soft yellow tint and full-height accent strip; the rail node is dashed-bordered yellow.
  </dd>
  <dt><span class="chip-doc chip-doc-purple">Activity <em>4</em></span></dt>
  <dd>System events: state changes (closed/merged/reopened/draft toggles), label edits, assignments, review requests, force-pushes, renames. Compact one-line rows with kind-colored icon nodes (state=red/purple/green by sub-type, commit=purple, label=blue, assign=green, review=orange, rename=yellow).</dd>
</dl>

<h4>Sanitisation</h4>
<ul class="prop-list">
  <li><strong>HTML comments stripped</strong><code>&lt;!-- policy_violation_comment --&gt;</code> and other invisible markers (used by the GitLab Security Bot, dependabot, etc.) are removed before rendering, so they no longer surface as literal text.</li>
  <li><strong>Emoji shortcodes</strong><code>:warning:</code> → ⚠️, <code>:white_check_mark:</code> → ✅, <code>:x:</code> → ❌, etc. ~90 shortcodes resolved (covers GitHub, GitLab and the common ecosystem aliases). Unknown shortcodes are left intact.</li>
  <li><strong>Activity body trimming</strong>GitLab system notes that ship with an HTML expansion ("added 83 commits<code>&lt;ul&gt;…&lt;/ul&gt;</code>") are truncated at the first tag — the timeline shows just the human-readable lede.</li>
</ul>

<h4>Default visibility</h4>
<p>Configure which chips start visible from <strong>Settings → Access → Merge Requests</strong>. Defaults are stored in <code>~/.config/arbor/config.toml</code> under <code>[mr]</code>:</p>
<pre class="language-toml">{@html highlight(`[mr]
default_show_comments = true
default_show_bots     = true
default_show_activity = true`, 'toml')}</pre>
<p>Toggling a chip inside an open modal is session-only — it never writes back to the config. Use Settings to change the global default.</p>

<h3>Closing a PR / MR</h3>
<p>The <strong>Close</strong> button (visible when the PR/MR is open) asks for explicit confirmation in a centred dialog before sending the close request — no more "I clicked it thinking I was closing the modal" mistakes. The dialog spells out which number is about to be closed.</p>

<h3>CI tab</h3>
<p>Reuses the same GitHub Actions / GitLab CI integration as the <strong>Pipelines</strong> panel, scoped to the source branch of this PR/MR.</p>
<dl class="meta-grid">
  <dt>Provider header</dt>
  <dd>Shows the detected provider (<em>GitHub Actions</em> or <em>GitLab CI</em>), the source branch chip, and a refresh button. Hidden when no remote is detected or no token is connected.</dd>
  <dt>Run cards</dt>
  <dd>Status pill (Passed / Failed / Running / Cancelled / Pending), wall-clock duration, run name + numeric id, short commit SHA, time-ago.</dd>
  <dt>PR HEAD pill</dt>
  <dd>The run whose commit SHA matches the current PR head is marked with an accent <em>PR HEAD</em> pill and an accent border, so the run that built the latest push stands out.</dd>
  <dt>Re-trigger</dt>
  <dd>Per-row button. Calls <code>POST /actions/runs/{'{id}'}/rerun</code> on GitHub or <code>POST /pipelines/{'{id}'}/retry</code> on GitLab, then reloads the list.</dd>
  <dt>Open in browser</dt>
  <dd>Per-row link to the run's web page on the provider.</dd>
  <dt>Detail modal</dt>
  <dd>Click a card to open the full stage / job graph — same modal used from the Pipelines panel. The provider icon is brand-tinted (orange for GitLab); stages render left-to-right in execution order; <kbd>Esc</kbd> closes.</dd>
</dl>
<div class="hint">
  Authentication is shared with the CI/CD panel — connect your GitHub or GitLab
  account once in <strong>Settings → Authentication</strong> and the CI tab
  picks the same token up. Self-hosted GitLab instances are supported.
</div>

<h4>How runs are discovered</h4>
<p>Both providers can attach pipeline runs to a PR/MR via paths a plain branch filter would miss. To catch all of them Arbor hits two endpoints per provider in parallel and deduplicates by run id (newest first).</p>
<dl class="meta-grid">
  <dt>GitHub</dt>
  <dd>
    <ul class="prop-list">
      <li><strong>Branch query</strong><code>GET /actions/runs?branch={'{source_branch}'}</code> — push and <code>pull_request</code> runs whose <code>head_branch</code> matches.</li>
      <li><strong>Head-SHA query</strong><code>GET /actions/runs?head_sha={'{head_sha}'}</code> — fork PRs, <code>pull_request_target</code> workflows, and <code>workflow_dispatch</code> runs pinned to the SHA. These don't always tag the source branch on the run.</li>
    </ul>
  </dd>
  <dt>GitLab</dt>
  <dd>
    <ul class="prop-list">
      <li><strong>Detached MR pipelines</strong><code>GET /merge_requests/:iid/pipelines</code> — required for pipelines whose <code>ref</code> is <code>refs/merge-requests/{'{iid}'}/head</code>. These are the ones GitLab shows at the top of the MR page as <em>"Merge request pipeline #..."</em> and would otherwise be invisible to a plain branch filter.</li>
      <li><strong>Branch pipelines</strong><code>GET /pipelines?ref={'{source_branch}'}</code> — regular pushes to the source branch.</li>
    </ul>
  </dd>
</dl>

<h3>Merge options</h3>
<p>The merge button is a split button. Click the main area for the default strategy, or the chevron for:</p>
<dl class="meta-grid">
  <dt>Merge commit</dt><dd>Creates a merge commit <span class="badge badge-opt">default</span></dd>
  <dt>Squash and merge</dt><dd>Squashes all commits into one</dd>
  <dt>Rebase and merge</dt><dd>Rebases onto the target branch <span class="badge badge-accent">GitHub only</span></dd>
</dl>
<p>Two checkboxes sit next to the split button and apply to whichever strategy you pick:</p>
<dl class="meta-grid">
  <dt>Squash</dt><dd>Collapse all commits of the branch into one before merging.</dd>
  <dt>Delete branch</dt>
  <dd>
    Remove the source branch on the remote and also clean up the local copy.
    Local deletion is guarded: if the source branch is currently checked out
    (in this repo or any worktree) Arbor first tries to switch to the target,
    and when that's not possible it keeps the branch and posts a warning in
    the notifications bell explaining what to do.
  </dd>
</dl>

<h4>Local-cleanup safety rules</h4>
<p>When <strong>Delete branch</strong> is ticked, Arbor only removes the local copy of the source branch after all these conditions are met:</p>
<ul class="prop-list">
  <li><strong>Branch exists locally</strong>Nothing to do if you never had it — the step is a no-op.</li>
  <li><strong>No worktree is using it</strong>A linked worktree holding the branch blocks deletion. Arbor notifies with the worktree path so you can remove it first.</li>
  <li><strong>HEAD switched away</strong>If the source branch is the current branch, Arbor checks out the target before deleting. A dirty workdir or a missing local target aborts the cleanup with a warning.</li>
</ul>
<div class="hint">
  The button is disabled while the merge status is being checked. When
  conflicts are detected it is replaced by <em>Resolve Conflicts</em>, which
  fetches <code>origin</code>, checks out the source branch, and merges the
  target into it so you can finish the merge locally.
</div>

<h2>Creating a PR / MR</h2>
<p>Click <strong>+</strong> in the sidebar header.</p>
<dl class="meta-grid">
  <dt>Title <span class="badge badge-req">Req</span></dt><dd>Summary shown in the list.</dd>
  <dt>Source branch</dt><dd>Defaults to the current branch of the active repo.</dd>
  <dt>Target branch</dt><dd>The branch to merge into (e.g. <code>main</code>).</dd>
  <dt>Description</dt><dd>Optional markdown text.</dd>
  <dt>Labels</dt><dd>Comma-separated label names.</dd>
  <dt>Draft</dt><dd>Mark the PR/MR as a draft / work in progress.</dd>
  <dt>Auto-merge</dt>
  <dd>
    Arm the platform's auto-merge when the PR/MR is opened. The platform
    merges once required checks pass — GitHub uses <em>auto-merge</em>
    (requires branch protection on the target branch), GitLab uses <em>merge
    when pipeline succeeds</em>. If it can't be armed, a notification is
    posted in the bell; the PR/MR itself is still created.
  </dd>
</dl>

<p>Merge strategy and source-branch deletion are chosen at merge time from the detail modal, not here.</p>

<h2>Supported features</h2>
<table class="matrix">
  <thead><tr><th>Feature</th><th>GitHub</th><th>GitLab</th></tr></thead>
  <tbody>
    <tr><td>List open / closed / merged</td><td class="yes">✓</td><td class="yes">✓</td></tr>
    <tr><td>Sidebar search (client-side)</td><td class="yes">✓</td><td class="yes">✓</td></tr>
    <tr><td>Markdown description &amp; comments</td><td class="yes">✓</td><td class="yes">✓</td></tr>
    <tr><td>Emoji shortcodes (<code>:warning:</code> → ⚠️)</td><td class="yes">✓</td><td class="yes">✓</td></tr>
    <tr><td>Activity timeline (state / labels / assigns / …)</td><td class="yes">✓ <em>via /events</em></td><td class="yes">✓ <em>via system notes</em></td></tr>
    <tr><td>Bot detection (filterable)</td><td class="yes">✓ <em>[bot] suffix</em></td><td class="yes">✓ <em>name heuristic</em></td></tr>
    <tr><td>Create PR / MR</td><td class="yes">✓</td><td class="yes">✓</td></tr>
    <tr><td>Auto-merge on creation</td><td class="yes">✓ <em>branch protection req.</em></td><td class="yes">✓ merge-when-pipeline-succeeds</td></tr>
    <tr><td>Merge</td><td class="yes">✓ merge / squash / rebase</td><td class="yes">✓ merge / squash</td></tr>
    <tr><td>Delete source branch on merge</td><td class="yes">✓</td><td class="yes">✓</td></tr>
    <tr><td>Close (with confirmation) / Reopen</td><td class="yes">✓</td><td class="yes">✓</td></tr>
    <tr><td>Add comment</td><td class="yes">✓</td><td class="yes">✓</td></tr>
    <tr><td>Draft / WIP flag</td><td class="yes">✓</td><td class="yes">✓</td></tr>
    <tr><td>Labels</td><td class="yes">✓</td><td class="yes">✓</td></tr>
    <tr><td>Assignees / Reviewers</td><td class="yes">✓</td><td class="yes">✓</td></tr>
    <tr><td>CI checks summary (Overview)</td><td class="partial">when available</td><td class="no">—</td></tr>
    <tr><td>Pipeline runs tab (filtered by source branch)</td><td class="yes">✓</td><td class="yes">✓</td></tr>
    <tr><td>Re-trigger run from PR/MR</td><td class="yes">✓</td><td class="yes">✓</td></tr>
    <tr><td>Self-hosted instance</td><td class="no">—</td><td class="yes">✓</td></tr>
  </tbody>
</table>

<h2>Plugin hooks</h2>
<p>Declare the hook booleans in <code>[hooks]</code> and register handlers in Lua.</p>
<pre class="language-toml">{@html highlight(`# plugin.toml
[hooks]
on_mr_opened  = true
on_mr_merged  = true`, 'toml')}</pre>
<pre class="language-lua">{@html highlight(`-- main.lua
arbor.events.on("on_mr_opened", function(ctx)
  arbor.notify{ title = "PR opened", message = "#" .. ctx.number .. ": " .. ctx.title, level = "info" }
end)

arbor.events.on("on_mr_merged", function(ctx)
  arbor.notify{ title = "PR merged", message = "#" .. ctx.number .. " was merged", level = "success" }
end)`, '.lua')}</pre>

<h3>Hook reference</h3>
<table>
  <thead><tr><th>Hook</th><th>Constant</th><th>Context</th></tr></thead>
  <tbody>
    <tr>
      <td><code>on_mr_opened</code></td>
      <td><code>hooks.MR_OPENED</code></td>
      <td><code>number, title, source_branch, target_branch, author, provider, web_url</code></td>
    </tr>
    <tr>
      <td><code>on_mr_merged</code></td>
      <td><code>hooks.MR_MERGED</code></td>
      <td><code>number, provider</code></td>
    </tr>
    <tr>
      <td><code>on_mr_updated</code></td>
      <td><code>hooks.MR_UPDATED</code></td>
      <td><code>number, provider</code> <span class="badge badge-beta">future use</span></td>
    </tr>
  </tbody>
</table>

<style>
  /* Mini chips used in the Activity-timeline section to mirror the modal UI */
  .chip-doc {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    padding: 1px 9px 1px 8px;
    border-radius: 99px;
    font-size: 10.5px;
    font-weight: 600;
    border: 1px solid;
    line-height: 1.5;
  }
  .chip-doc em {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    min-width: 16px;
    padding: 0 4px;
    background: var(--bg-overlay);
    border-radius: 99px;
    font-size: 9.5px;
    font-style: normal;
    font-weight: 700;
  }
  .chip-doc-blue {
    background: color-mix(in srgb, var(--accent) 14%, transparent);
    border-color: color-mix(in srgb, var(--accent) 45%, transparent);
    color: var(--accent);
  }
  .chip-doc-yellow {
    background: color-mix(in srgb, var(--warning) 14%, transparent);
    border-color: color-mix(in srgb, var(--warning) 45%, transparent);
    color: var(--warning);
  }
  .chip-doc-purple {
    background: color-mix(in srgb, var(--color-tag) 14%, transparent);
    border-color: color-mix(in srgb, var(--color-tag) 45%, transparent);
    color: var(--color-tag);
  }
</style>
