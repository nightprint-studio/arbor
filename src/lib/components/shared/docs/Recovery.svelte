<script lang="ts">
  import Callout from '$lib/components/shared/ui/Callout.svelte';
</script>

<h1>Recovery Journal</h1>

<p class="doc-lead">
  The <strong>Recovery Journal</strong> is Arbor's automatic safety net — before every destructive
  git operation, a full snapshot of your working tree and index is saved as an unreachable git
  object and logged in <code>.git/arbor-recovery/journal.jsonl</code>.
  If something goes wrong you can browse and restore any snapshot with one click.
</p>

<h2>What triggers a snapshot</h2>
<p>Snapshots are created automatically — no action required — before:</p>
<div class="feature-grid two-col">
  <div class="feature-card">
    <div class="fc-title">Reset —hard</div>
    <div class="fc-desc">Any hard reset of HEAD or the index, including interactive rebase steps.</div>
  </div>
  <div class="feature-card">
    <div class="fc-title">Checkout</div>
    <div class="fc-desc">Branch or commit checkout that modifies tracked files in the working tree.</div>
  </div>
  <div class="feature-card">
    <div class="fc-title">Discard changes</div>
    <div class="fc-desc">"Discard file" or "Discard all changes" from the Stage panel.</div>
  </div>
  <div class="feature-card">
    <div class="fc-title">Stash force-apply</div>
    <div class="fc-desc">Force-applying a stash over conflicting untracked files.</div>
  </div>
  <div class="feature-card">
    <div class="fc-title">Stash drop</div>
    <div class="fc-desc">Dropping a stash entry manually from the Stash panel.</div>
  </div>
  <div class="feature-card">
    <div class="fc-title">Other</div>
    <div class="fc-desc">Any operation not in the above categories that may overwrite work.</div>
  </div>
</div>

<Callout variant="info">
  Snapshots are taken <strong>before</strong> the operation runs, so even if the operation fails
  mid-way you still have a clean restore point.
</Callout>

<h2>Opening the Recovery tab</h2>
<p>
  Click the <strong>History</strong> icon (clock-arrow) in the Activity Bar to open the Reflog
  sidebar, then switch to the <strong>Recovery</strong> tab at the top.
</p>

<h2>Reading a recovery entry</h2>
<table class="shortcuts-table">
  <thead><tr><th>Element</th><th>Meaning</th></tr></thead>
  <tbody>
    <tr>
      <td><code>shield</code> badge + kind label</td>
      <td>Type of operation that triggered the snapshot (Checkout, Reset·hard, Discard, etc.)</td>
    </tr>
    <tr>
      <td>Summary line</td>
      <td>Human-readable description, e.g. <em>checkout branch 'feature/x'</em></td>
    </tr>
    <tr>
      <td>Relative time</td>
      <td>When the snapshot was taken; hover for the full date/time</td>
    </tr>
    <tr>
      <td>File-warning icon</td>
      <td>Some files were too large or had denied extensions and were <em>logged but not preserved</em></td>
    </tr>
    <tr>
      <td>Consumed badge</td>
      <td>Entry has been restored; the pinning ref has been removed</td>
    </tr>
  </tbody>
</table>

<h2>Preview &amp; Restore</h2>
<p>
  Click any entry to expand it and see a <strong>preview diff</strong> — the list of files
  that would change if you restored that snapshot from the current state.
</p>
<p>
  Click <strong>Restore</strong> to apply the snapshot via <code>git stash apply</code>.
  Arbor always uses <em>apply</em> (never pop) so the snapshot is preserved in case the apply
  produces conflicts. Once the apply is clean, the pinning ref is automatically released.
</p>

<Callout variant="warning">
  Restoring a snapshot overwrites your current working tree. Arbor takes a new safety snapshot
  <em>before</em> each restore, so the operation is always reversible.
</Callout>

<h2>Deleting entries</h2>
<p>
  Use the trash icon on an entry to remove it. This drops the pinning
  <code>refs/arbor/recovery/…</code> ref — the objects become eligible for git garbage
  collection after the standard unreachable-object grace period.
</p>

<h2>Automatic expiry</h2>
<p>
  Entries older than the configured <strong>retention period</strong> (default: <strong>30 days</strong>)
  are pruned lazily each time the recovery list is loaded. You can adjust the retention period and
  other limits in <strong>Settings → Performance → Recovery</strong>.
</p>

<h2>Reflog vs. Recovery Journal</h2>
<table class="shortcuts-table">
  <thead>
    <tr><th></th><th>Reflog</th><th>Recovery Journal</th></tr>
  </thead>
  <tbody>
    <tr>
      <td><strong>What it tracks</strong></td>
      <td>Every position of HEAD — commits, checkouts, merges, rebases</td>
      <td>Working-tree + index snapshots before destructive ops</td>
    </tr>
    <tr>
      <td><strong>Uncommitted work</strong></td>
      <td>Not preserved — only the committed state</td>
      <td>Fully preserved (working dir + staged changes)</td>
    </tr>
    <tr>
      <td><strong>Managed by</strong></td>
      <td>Git itself</td>
      <td>Arbor exclusively</td>
    </tr>
    <tr>
      <td><strong>When to use</strong></td>
      <td>Recover a lost <em>commit</em> after reset or force-push</td>
      <td>Recover <em>uncommitted</em> work after a discard or checkout</td>
    </tr>
  </tbody>
</table>

<h2>Settings</h2>
<p>Configure the journal in <strong>Settings → Performance → Recovery</strong>:</p>
<table class="shortcuts-table">
  <thead><tr><th>Setting</th><th>Default</th><th>Effect</th></tr></thead>
  <tbody>
    <tr>
      <td>Max file size</td>
      <td>2 MB</td>
      <td>Files larger than this limit are <em>logged</em> in the journal but their content is not preserved in the snapshot.</td>
    </tr>
    <tr>
      <td>Retention period</td>
      <td>30 days</td>
      <td>Snapshots older than this are pruned on next load. Matches git's default unreachable-object expiry.</td>
    </tr>
    <tr>
      <td>Denied extensions</td>
      <td>zip, mp4, exe, dll, jar, psd, …</td>
      <td>Files with these extensions are never content-preserved — only logged. Avoids bloating <code>.git</code> with build artifacts and binaries.</td>
    </tr>
  </tbody>
</table>

<h2>Under the hood</h2>
<p>
  Snapshots use the same mechanism as <code>git stash create</code> — they produce a commit
  containing a tree of the working directory with a separate parent tree for the index.
  Unlike a real stash, the commit is <strong>not</strong> pushed to <code>refs/stash</code>.
  Instead, it is pinned under a dedicated namespace:
</p>
<pre><code>refs/arbor/recovery/&lt;id&gt;-&lt;kind&gt;</code></pre>
<p>
  This keeps the objects alive through garbage collection until Arbor's TTL expires and the
  ref is explicitly removed. The journal itself is stored as an append-only JSONL file at:
</p>
<pre><code>.git/arbor-recovery/journal.jsonl</code></pre>
<p>
  Each line is a self-contained JSON object with the fields below.
</p>
<table class="shortcuts-table">
  <thead><tr><th>Field</th><th>Type</th><th>Description</th></tr></thead>
  <tbody>
    <tr><td><code>id</code></td><td><code>u64</code></td><td>Monotonically-increasing unique identifier</td></tr>
    <tr><td><code>created_at</code></td><td><code>i64</code></td><td>Unix timestamp of snapshot creation</td></tr>
    <tr><td><code>kind</code></td><td><code>string</code></td><td>One of: <code>reset_hard</code>, <code>checkout</code>, <code>discard</code>, <code>stash_force_apply</code>, <code>stash_drop</code>, <code>pull</code>, <code>other</code></td></tr>
    <tr><td><code>summary</code></td><td><code>string</code></td><td>Human-readable description of the triggering operation</td></tr>
    <tr><td><code>snapshot_oid</code></td><td><code>string</code></td><td>Full OID of the stash-create commit (null if snapshot was skipped)</td></tr>
    <tr><td><code>head_oid</code></td><td><code>string</code></td><td>OID of HEAD at snapshot time</td></tr>
    <tr><td><code>head_branch</code></td><td><code>string | null</code></td><td>Branch name at snapshot time (null for detached HEAD)</td></tr>
    <tr><td><code>consumed</code></td><td><code>bool</code></td><td>True after the entry has been successfully restored</td></tr>
    <tr><td><code>skipped_files</code></td><td><code>array</code></td><td>Files that were logged but not preserved (too large or denied extension)</td></tr>
  </tbody>
</table>
