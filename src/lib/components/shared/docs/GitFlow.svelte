<script lang="ts">
  import { highlight } from '$lib/utils/diff-formatter';
</script>

<h1>Git Flow</h1>
<p class="doc-lead">Arbor includes a built-in Git Flow implementation based on the <strong>Vincent Driessen branching model</strong> â€” structured workflows for feature, release, and hotfix branches, with optional PR/MR integration and ticket-based branch naming.</p>

<h2>Opening the Git Flow panel</h2>
<p>Click the <strong>Git Merge</strong> icon (second icon) in the Activity Bar to open the Git Flow sidebar panel.</p>

<h2>Initialization</h2>
<p>If the repository has never been initialized with Git Flow, the panel shows an <strong>Initialize</strong> button. This creates the <code>develop</code> branch (if it doesn't exist) and records the prefix configuration. Branch prefixes default to:</p>

<h3>Non-standard flow (no <code>develop</code>)</h3>
<p>Arbor works with repositories that <strong>don't follow the standard <code>main</code>/<code>develop</code> split</strong>. When <code>main</code> exists but <code>develop</code> doesn't, the panel is fully usable and you can still create feature/release branches â€” they are created from <code>main</code> instead of <code>develop</code>, and finishing them merges back into <code>main</code>.</p>
<ul>
  <li>A yellow <strong>"Non-standard flow"</strong> banner is shown at the top of the panel in this mode. It carries an <strong>Initialise</strong> shortcut that creates the missing <code>develop</code> branch from <code>main</code> if you want to switch to the full Git Flow.</li>
  <li>The <strong>first time</strong> you start a feature or release in this mode for a given project, a confirmation dialog explains that the branch will be cut from <code>main</code>. Confirming the dialog stores an acknowledgement per project â€” subsequent starts go through silently.</li>
  <li>A toast after the start reminds you which base branch was used (e.g. <em>"feature 'foo' started from main"</em>).</li>
  <li>If neither <code>main</code> nor <code>develop</code> exists, the panel falls back to the standard "create main" flow before anything else can be done.</li>
</ul>
<table class="shortcuts-table">
  <thead><tr><th>Branch type</th><th>Default prefix</th></tr></thead>
  <tbody>
    <tr><td>feature</td><td><code>feature/</code></td></tr>
    <tr><td>release</td><td><code>release/</code></td></tr>
    <tr><td>hotfix</td><td><code>hotfix/</code></td></tr>
    <tr><td>bugfix</td><td><code>bugfix/</code></td></tr>
    <tr><td>support</td><td><code>support/</code></td></tr>
  </tbody>
</table>

<h2>Workflows</h2>

<h3>Feature branches</h3>
<ul>
  <li><strong>Start</strong> â€” creates <code>feature/&lt;name&gt;</code> from <code>develop</code> (or from <code>main</code> if <code>develop</code> doesn't exist) and checks it out</li>
  <li><strong>Finish</strong> â€” merges feature branch into <code>develop</code> with <code>--no-ff</code> (or into <code>main</code> when <code>develop</code> is missing); optionally deletes the branch after</li>
</ul>

<h3>Release branches</h3>
<ul>
  <li><strong>Start</strong> â€” creates <code>release/&lt;version&gt;</code> from <code>develop</code> (falls back to <code>main</code> when <code>develop</code> is missing)</li>
  <li><strong>Finish</strong> â€” merges into <code>main</code> and, when present, into <code>develop</code>; optionally creates a version tag</li>
</ul>

<h3>Hotfix branches</h3>
<ul>
  <li><strong>Start</strong> â€” creates <code>hotfix/&lt;name&gt;</code> from <code>main</code> (the production branch)</li>
  <li><strong>Finish</strong> â€” merges into both <code>main</code> and <code>develop</code>; optionally creates a tag</li>
</ul>

<h2>PR / MR integration</h2>
<p>Arbor supports both local merges and Pull / Merge Request workflows. The behaviour is controlled by two settings per branch type:</p>
<table class="shortcuts-table">
  <thead><tr><th>Setting</th><th>What it does</th></tr></thead>
  <tbody>
    <tr><td><code>finish.feature_use_pr</code></td><td><strong>Force</strong> PR/MR â€” feature finish always pushes the branch and opens the PR/MR form (no local merge)</td></tr>
    <tr><td><code>finish.feature_pr_default</code></td><td>When not forced, sets the <strong>default action</strong> for the primary Finish button. <code>false</code> (default) = merge locally; <code>true</code> = open PR/MR</td></tr>
    <tr><td><code>finish.release_use_pr</code></td><td><strong>Force</strong> PR/MR on release finish</td></tr>
    <tr><td><code>finish.release_pr_default</code></td><td>Default primary button action for release finish</td></tr>
    <tr><td><code>finish.hotfix_use_pr</code></td><td><strong>Force</strong> PR/MR on hotfix finish</td></tr>
    <tr><td><code>finish.hotfix_pr_default</code></td><td>Default primary button action for hotfix finish</td></tr>
  </tbody>
</table>
<p>When a finish type is <strong>not</strong> forced, the Finish button becomes a <strong>split button</strong>: the primary click uses the configured default, and the chevron <code>â–¾</code> lets you choose between "Finish normally (merge locally)" and "Finish with PR/MR" for that individual operation.</p>
<p>Configure in <strong>Settings â†’ Git Flow</strong>. Each setting can be overridden per project.</p>

<h2>Ticket-based branch naming</h2>
<p>When an issue tracker is configured for the project (see <strong>Settings â†’ Repository â†’ Issue Tracker</strong>), the "Start Feature" form shows a <strong>Ticket</strong> field with a picker button. Clicking it opens a full-screen modal with the same search and filter interface as the Issues sidebar â€” search bar, status / project / milestone / assignee chips â€” and issue cards with colored status icons, labels, and assignees.</p>
<p>Selecting a ticket closes the modal and auto-fills the branch name field with the ticket identifier, producing branches like <code>feature/ABO-123</code>.</p>
<ul>
  <li>The ticket picker is available <strong>by default</strong> whenever a tracker is configured â€” no flag required.</li>
  <li>Enable <code>require_ticket_branch</code> to make ticket selection <strong>mandatory</strong> (the branch name field must be filled from a ticket).</li>
  <li>If <code>require_ticket_branch</code> is on but no issue tracker is configured for the project, a warning is shown and the branch name can be typed freely.</li>
  <li>Currently supported tracker: <strong>Linear</strong>. Jira coming soon.</li>
</ul>

<h2>Configuration</h2>
<p>Git Flow settings are stored in two layers:</p>
<ul>
  <li><strong>Global</strong> â€” in <code>~/.config/arbor/config.toml</code> under <code>[gitflow]</code> â€” applies to all repositories</li>
  <li><strong>Per-repo</strong> â€” in <code>&lt;repo&gt;/.arbor/config.toml</code> under <code>[gitflow]</code> â€” overrides the global config for that repo only</li>
</ul>
<p>Both layers are editable from <strong>Settings â†’ Git Flow</strong>.</p>
<pre class="language-toml">{@html highlight(`[gitflow]
main_branch            = "main"      # or "master"
develop_branch         = "develop"
require_ticket_branch  = false       # force ticket-based branch names on feature start

[gitflow.prefixes]
feature = "feature/"
release = "release/"
hotfix  = "hotfix/"
bugfix  = "bugfix/"
support = "support/"

[gitflow.finish]
feature_delete_branch = true   # delete feature branch after finish
feature_squash        = false  # squash commits on feature finish
release_tag           = true   # create a version tag on release finish
release_tag_prefix    = "v"    # tag prefix, e.g. "v1.2.0"
hotfix_tag            = true   # create a tag on hotfix finish
feature_use_pr        = false  # force PR/MR on feature finish (overrides default)
feature_pr_default    = false  # default button: false = merge, true = PR/MR
release_use_pr        = false  # force PR/MR on release finish
release_pr_default    = false  # default button for release finish
hotfix_use_pr         = false  # force PR/MR on hotfix finish
hotfix_pr_default     = false  # default button for hotfix finish`, 'toml')}</pre>

<h2>Plugin hooks</h2>
<p>Plugins can react to every Git Flow operation. Declare the hooks in <code>[hooks]</code> and register handlers with <code>arbor.events.on()</code>:</p>
<table class="shortcuts-table">
  <thead><tr><th>Hook constant</th><th>TOML key</th><th>Context fields</th></tr></thead>
  <tbody>
    <tr><td><code>FLOW_INIT</code></td><td><code>on_flow_init</code></td><td>repo</td></tr>
    <tr><td><code>FLOW_FEATURE_START</code></td><td><code>on_flow_feature_start</code></td><td>repo, name, branch, base_branch</td></tr>
    <tr><td><code>FLOW_FEATURE_FINISH</code></td><td><code>on_flow_feature_finish</code></td><td>repo, name, branch</td></tr>
    <tr><td><code>FLOW_RELEASE_START</code></td><td><code>on_flow_release_start</code></td><td>repo, version, branch, base_branch</td></tr>
    <tr><td><code>FLOW_RELEASE_FINISH</code></td><td><code>on_flow_release_finish</code></td><td>repo, version, branch</td></tr>
    <tr><td><code>FLOW_HOTFIX_START</code></td><td><code>on_flow_hotfix_start</code></td><td>repo, name, branch, base_branch</td></tr>
    <tr><td><code>FLOW_HOTFIX_FINISH</code></td><td><code>on_flow_hotfix_finish</code></td><td>repo, name, branch</td></tr>
  </tbody>
</table>
<pre class="language-lua">{@html highlight(`-- plugin.toml [hooks] section
-- on_flow_feature_start = true
-- on_flow_feature_finish = true

arbor.events.on("on_flow_feature_start", function(ctx)
  -- ctx.repo   = "/path/to/repo"
  -- ctx.name   = "my-feature"    (name part only, without prefix)
  -- ctx.branch = "feature/my-feature"  (full branch name)
  arbor.log.info("Feature started: " .. ctx.branch)
end)

arbor.events.on("on_flow_feature_finish", function(ctx)
  arbor.notify{ title = "Feature merged", message = ctx.branch .. " merged into develop", level = "success" }
end)`, '.lua')}</pre>
