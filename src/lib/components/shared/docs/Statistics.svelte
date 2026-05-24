<script lang="ts">
  import Callout from '$lib/components/shared/ui/Callout.svelte';
</script>

<h1>Repository Statistics</h1>

<p class="doc-lead">
  Arbor can compute a detailed statistical profile of any open repository —
  commit activity, contributor breakdown, file hotspots, and more.
  All computation runs in a background thread so the UI stays responsive.
</p>

<h2>Opening Statistics</h2>
<ul>
  <li>Click the <strong>Bar Chart</strong> icon in the Activity Bar (left rail) to open the <strong>Stats sidebar panel</strong>.</li>
  <li>Click <strong>Full Statistics</strong> at the bottom of the panel to open the full-screen overlay.</li>
  <li>The overlay has three tabs: <strong>Overview</strong>, <strong>Contributors</strong>, <strong>Files</strong>.</li>
</ul>

<h2>Stats Sidebar Panel</h2>
<p>Shows a compact at-a-glance summary while you work:</p>
<div class="feature-grid two-col">
  <div class="feature-card">
    <div class="fc-title">Summary Cards</div>
    <div class="fc-desc">Four cards in a 2×2 grid: total commits, contributors, repository age, and active days. Each card has a coloured icon for quick identification.</div>
  </div>
  <div class="feature-card">
    <div class="fc-title">Commits / Week Sparkline</div>
    <div class="fc-desc">A 12-week bar chart showing weekly commit frequency. Includes a Y-axis scale (peak → 0) and a timeline (12w ago → now).</div>
  </div>
  <div class="feature-card">
    <div class="fc-title">Top Contributor</div>
    <div class="fc-desc">The all-time leader by commit count, with a percentage bar.</div>
  </div>
  <div class="feature-card">
    <div class="fc-title">Highlights</div>
    <div class="fc-desc">
      Four quick highlights:<br>
      • <strong>This week</strong> — top author in the last 7 days<br>
      • <strong>This month</strong> — top author in the last 30 days<br>
      • <strong>Most lines changed</strong> — author with the highest total line churn<br>
      • <strong>Longest streak</strong> — consecutive days with at least one commit
    </div>
  </div>
</div>

<h2>Overview Tab</h2>
<p>Summary cards available in the full overlay:</p>
<div class="props-table">
  <div class="prop-row"><span class="prop-name">Total Commits</span><span class="prop-desc">All commits reachable from HEAD.</span></div>
  <div class="prop-row"><span class="prop-name">Contributors</span><span class="prop-desc">Number of unique author emails.</span></div>
  <div class="prop-row"><span class="prop-name">Repository Age</span><span class="prop-desc">Time between first and last commit.</span></div>
  <div class="prop-row"><span class="prop-name">Active Days</span><span class="prop-desc">Calendar days that had at least one commit.</span></div>
  <div class="prop-row"><span class="prop-name">Avg / Week</span><span class="prop-desc">Commits per calendar week over the project lifetime.</span></div>
  <div class="prop-row"><span class="prop-name">Longest Streak</span><span class="prop-desc">Longest run of consecutive days with at least one commit.</span></div>
  <div class="prop-row"><span class="prop-name">Avg Commit Size</span><span class="prop-desc">Average lines changed (insertions + deletions) per commit, sampled from the first 500 commits.</span></div>
  <div class="prop-row"><span class="prop-name">First / Last Commit</span><span class="prop-desc">Dates of the oldest and newest commits.</span></div>
  <div class="prop-row"><span class="prop-name">Busiest Day</span><span class="prop-desc">The calendar date with the most commits, with count.</span></div>
</div>

<p>The Overview tab also includes:</p>
<ul>
  <li><strong>Commit Activity Heatmap</strong> — GitHub-style 52×7 calendar for the last 12 months. Hover a cell to see the exact count for that day.</li>
  <li><strong>Commit Timing</strong> — two bar charts: commits by hour of day (0–23) and commits by day of week (Mon–Sun). Both support hover tooltips on every column, even very small bars.</li>
</ul>

<h2>Contributors Tab</h2>
<p>Two ranked lists:</p>
<ul>
  <li><strong>By Commits</strong> — top 10 authors by commit count, with avatar initials, percentage bar, and commit count. Avatars use a deterministic hue derived from the author's email.</li>
  <li><strong>By Lines Changed</strong> — top 10 authors by total lines touched (insertions + deletions), sampled from the first 500 commits. Each row shows <code>+additions</code> and <code>−deletions</code> colour-coded pills and a two-tone bar split between adds (green) and deletes (red).</li>
</ul>

<h2>Files Tab</h2>
<ul>
  <li><strong>By File Type</strong> — top 10 extensions by cumulative change count, shown as coloured horizontal bars.</li>
  <li><strong>Most Changed Files</strong> — top 20 individual files by change count, sampled from the first 500 commits.</li>
</ul>

<Callout variant="info" title="Performance note:">
  File-level and contributor line stats are sampled from the <strong>first 500 commits</strong> for performance. Commit-level stats (totals, contributors by count, timing, heatmap) scan the full history.
</Callout>

<h2>Caching</h2>
<p>
  Results are cached in memory keyed by the current HEAD SHA <em>and</em> your exclusion settings.
  The cache is invalidated automatically when you push a new commit or change the exclusion config.
  Click <strong>Recompute</strong> (↻ button in the panel header) to force a fresh calculation.
</p>

<h2>Exporting Statistics</h2>
<p>
  The statistics overlay header provides two export buttons — <strong>JSON</strong> and <strong>HTML</strong> — visible whenever stats have been computed.
  Click either button to open a file-picker dialog and choose the output location.
</p>
<div class="feature-grid two-col">
  <div class="feature-card">
    <div class="fc-title">JSON Export</div>
    <div class="fc-desc">
      Pretty-printed JSON file mirroring the full <code>RepoStats</code> struct.
      Includes all numeric arrays (commits by hour/weekday, heatmap), top contributors, top files, and file type breakdown.
      Useful for scripting or importing into other tools.
    </div>
  </div>
  <div class="feature-card">
    <div class="fc-title">HTML Report</div>
    <div class="fc-desc">
      A fully self-contained HTML file — no external dependencies.
      Includes the <strong>Arbor logo</strong>, inline dark-theme CSS, and inline SVG charts:
      commit heatmap, hour/weekday distributions, contributor bars (by commits and by lines), and file type breakdown.
      All charts support <strong>hover tooltips</strong> (date + count on heatmap cells; label + count on hour and weekday bars).
      Opens in any browser.
    </div>
  </div>
</div>
<Callout variant="info">
  The export runs as a <strong>background job</strong> so the UI stays responsive.
  Progress and completion status are visible in the <em>Jobs</em> overlay, and a bell notification appears when the export finishes or fails.
  If statistics have already been computed for the current HEAD, the cached data is used directly — no re-computation is needed.
</Callout>

<h2>Excluding Files from Statistics</h2>
<p>
  Go to <strong>Settings → Project → Statistics</strong> to configure per-repository exclusions.
  Excluded paths are ignored in the file-level charts (Most Changed Files, By File Type) but do not affect commit-level stats.
</p>
<div class="props-table">
  <div class="prop-row"><span class="prop-name">Extensions</span><span class="prop-desc">File extensions to ignore — e.g. <code>ron</code>, <code>lock</code>. Enter without the leading dot.</span></div>
  <div class="prop-row"><span class="prop-name">Folders</span><span class="prop-desc">Folder prefixes to exclude — e.g. <code>assets/generated</code>, <code>vendor</code>. All files whose path starts with the prefix are skipped.</span></div>
  <div class="prop-row"><span class="prop-name">Files</span><span class="prop-desc">Exact file names or relative paths — e.g. <code>Cargo.lock</code>, <code>src/generated/schema.rs</code>.</span></div>
</div>
<p>Exclusions are stored in <code>.arbor/config.toml</code> inside the repository. After saving, click <strong>Recompute</strong> to apply them.</p>
