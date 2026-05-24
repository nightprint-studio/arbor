<script lang="ts">
  import Callout from '$lib/components/shared/ui/Callout.svelte';
  import Kbd     from '$lib/components/shared/internal/Kbd.svelte';
</script>

<h1>Merge Conflicts</h1>

<p class="doc-lead">When a merge produces conflicts, Arbor detects the state automatically and surfaces a guided resolution workflow. No need to manually edit <code>&lt;&lt;&lt;&lt;&lt;&lt;&lt;</code> markers in a text editor.</p>

<h2>Spotting a merge in progress</h2>
<p>Three entry points appear simultaneously:</p>
<ul>
  <li>The <strong>WIP node</strong> in the graph turns amber and shows a pill with the number of conflicted files. A <strong>Resolve</strong> button appears directly on the node.</li>
  <li>The <strong>Branches &amp; Stashes</strong> sidebar shows an amber banner: <em>"N file in conflitto — Risolvi conflitti…"</em></li>
  <li>The <strong>Stage Area</strong> shows a merge notice instead of the normal file lists, with a button to open the resolver.</li>
</ul>

<h2>Resolution modal layout</h2>
<p>Click any entry point to open the modal. It mirrors the main app's IntelliJ-style layout: a card-shaped <strong>file sidebar</strong> on the left and a <strong>two-column editor</strong> + <strong>result panel</strong> on the right.</p>

<div class="conflict-panels">
  <div class="cp-col cp-ours">Ours<br><small>nostro — your branch (HEAD)</small></div>
  <div class="cp-col cp-theirs">Theirs<br><small>loro — incoming branch</small></div>
</div>

<h3>File sidebar</h3>
<p>Lists every file flagged as conflicted at any point during the session. Each row is a card with three possible states:</p>
<ul class="prop-list">
  <li><span class="icon-conflict">⚠</span><strong>conflict</strong>regions still need a choice.</li>
  <li><span class="icon-ok">✓</span><strong>resolved</strong>composed result written and staged.</li>
  <li><strong>viewed</strong>opened but no decision yet (greyed badge).</li>
</ul>
<p>For <strong>modify/delete</strong> and <strong>add/modify</strong> entries the row gets a coloured pill — <em>"added by them"</em> or <em>"deleted by them"</em> — populated up front via <code>get_conflict_presence</code> so the sidebar can show the state without loading every file's three-way content.</p>

<p>The header carries:</p>
<ul class="prop-list">
  <li><strong>List ↔ Tree toggle</strong>switches the file list between flat and folder-grouped (same widget pair as the Stage panel). Folders collapse/expand independently.</li>
  <li><strong>Collapse</strong>circular chevron — hides the sidebar; when collapsed, an icon in the modal title bar reopens it.</li>
</ul>

<p><strong>Right-click a file</strong> for a fast-resolve menu:</p>
<ul>
  <li><strong>Prendi nostro (&lt;branch&gt;)</strong> — resolves every conflict region in that file by keeping the local side, then stages it.</li>
  <li><strong>Prendi loro (&lt;branch&gt;)</strong> — same but with the incoming side.</li>
</ul>
<p>Works on files you haven't opened yet — the conflict content is loaded on demand.</p>

<h3>Modify/delete &amp; add/modify resolver</h3>
<p>When one side <em>deleted</em> or <em>added</em> the file (so there's no overlapping content to merge line-by-line), Arbor swaps the two-column view for a dedicated resolver. The regular diff would mislead by duplicating context lines on both sides — there are no <code>&lt;&lt;&lt;&lt;&lt;&lt;&lt;</code> markers in the workdir for these cases.</p>
<ul class="prop-list">
  <li><strong>Banner</strong>"Added on &lt;branch&gt;" or "Deleted on &lt;branch&gt;" with a triangle-alert icon.</li>
  <li><strong>Two stacked cards</strong>
    <em>Keep file</em> — use the version from the side that still has it.<br>
    <em>Accept deletion</em> — remove the file from workdir and index (danger / red button).
  </li>
  <li><strong>Live preview</strong>shows either the file content that will be kept, or a "file will be removed" placeholder.</li>
</ul>

<h3>Conflict navigation toolbar</h3>
<p>A toolbar across the top of the editor area lets you jump between conflict blocks <em>inside the active file</em>:</p>
<ul class="prop-list">
  <li><strong>↑ / ↓</strong>step through regions (also bound to <Kbd action="next_chunk" /> / <Kbd action="prev_chunk" />).</li>
  <li><strong>Counter</strong>"<em>3 / 7</em>" — current region over total.</li>
  <li><strong>‹ ours</strong> / <strong>theirs ›</strong>resolve the active block and advance to the next.</li>
  <li><strong>"File staged"</strong> badge appears once every region is resolved and the result is written.</li>
</ul>

<h3>Two-column synchronized view</h3>
<p>Each side shows numbered lines. The column header carries the branch name plus a <strong>master checkbox</strong>: tick it to flag every line on that side across <em>all</em> conflict regions of the file at once. The checkbox shows an <em>indeterminate</em> state when the per-line selections are mixed.</p>

<p>Inside each conflict region you'll find:</p>
<ul>
  <li>A <strong>"Conflitto N"</strong> header with three small icon buttons on the right:
    <ul>
      <li><strong>‹</strong> — accept this region's <em>ours</em> (selects all ours lines, deselects theirs)</li>
      <li><strong>=</strong> — accept both (selects every line on both sides, ours first then theirs)</li>
      <li><strong>›</strong> — accept this region's <em>theirs</em></li>
    </ul>
    Branch labels live in the column headers above, so the per-region buttons stay compact.
  </li>
  <li><strong>Per-line checkboxes</strong> — for fine-grained mixing. Click a line to toggle it.</li>
</ul>

<p>Long context blocks (more than 30 lines) are <strong>clipped</strong> to the first and last 12 lines with a <em>"… N righe di contesto nascoste"</em> placeholder in the middle. Click it to expand. This keeps the modal responsive on huge files where rendering thousands of unchanged lines would otherwise freeze the UI.</p>

<h3>Result panel</h3>
<p>The bottom half of the editor area shows the <strong>computed result</strong> from your selections, syntax-highlighted. It's a real editable <code>textarea</code> — type directly to override the computed result; a <em>"modificato manualmente"</em> badge appears, and <em>↩ Ripristina</em> reverts back to the selection-driven version. The horizontal divider between the two-column view and the result is draggable to resize.</p>

<h3>Full file context</h3>
<p>The <strong>file icon</strong> in the modal header mirrors the global <em>Show full file</em> diff setting: when on, the conflict editor expands every collapsed context block at once instead of trimming long unchanged regions. Useful when the surrounding code matters for choosing between ours/theirs.</p>

<h3>Auto-staging</h3>
<p>As soon as <em>all conflict regions in a file are resolved</em>, Arbor writes the result to disk and stages the file automatically (equivalent to <code>git add &lt;file&gt;</code>). A green checkmark appears in the sidebar — no manual save step needed. Resolved files are remembered for the session even if git later removes them from the conflicted list.</p>

<h3>File encoding</h3>
<p>
  Legacy codebases (Java, PHP, <code>.properties</code> on Windows) often
  ship in <code>windows-1252</code> rather than UTF-8. Arbor sniffs the
  encoding from the working-tree bytes — UTF-8 BOM or strict UTF-8 →
  UTF-8, otherwise <code>windows-1252</code> as a lossless fallback.
  All three stages (ours / theirs / base) are decoded with the same
  encoding so the three-way view never mixes decoders mid-stream.
</p>
<p>
  An <strong>encoding pill</strong> sits in the modal header next to the
  branch chips: it shows the active label (e.g. <code>UTF-8</code>) and is
  clickable. Pick a different encoding from the dropdown
  (<em>UTF-8</em> / <em>windows-1252</em> / <em>ISO-8859-1</em> /
  <em>ISO-8859-15</em> / <em>MacRoman</em> / <em>windows-1250</em> /
  <em>Shift_JIS</em> / <em>GB18030</em> / <em>EUC-KR</em>) and the file
  reloads with that decoder. The pill takes a warning tint when an
  override is active; <em>Auto-detect</em> clears it.
</p>
<p>
  Overrides are persisted per <code>(repo, file)</code> in browser
  storage so the choice survives reloads. On save the resolved content
  is re-encoded back to the same byte representation — a windows-1252
  source stays windows-1252 on disk after resolution, never silently
  rewritten as UTF-8.
</p>
<p>
  The same pill appears in <strong>every diff viewer</strong> (Stage Area,
  Commit Detail, Branch Compare, Stash diff) so the same override applies
  consistently across surfaces.
</p>

<h3>Completing the merge</h3>
<p>Once every conflicted file is resolved, the <strong>Mergia →</strong> button in the footer activates. The commit message input is pre-filled from <code>.git/MERGE_MSG</code> with the auto-appended <code>Conflicts:</code> section stripped (the conflicted file list is already in the modal — repeating it in the commit message is noise). Edit if needed, then click to create the merge commit.</p>

<h3>Aborting the merge</h3>
<p>Click <strong>Annulla Merge</strong> in the footer (to the left of <em>Mergia</em>) to discard all resolution work. A confirmation prompt appears — confirm to run <code>git merge --abort</code> and restore the working tree.</p>

<Callout variant="warning" title="Abort is irreversible">
  Aborting discards all conflict resolutions you've made so far. You'll need to start over if you re-trigger the merge.
</Callout>

<h2>Blocking files (stash apply)</h2>
<p>When a <code>stash apply</code> / <code>pop</code> can't proceed because tracked or untracked files in the workdir would be overwritten, the same modal opens in <em>blocking-files</em> mode. The sidebar shows two clearly-separated sections:</p>
<ul class="prop-list">
  <li><strong>Conflicts</strong>regular conflicting tracked files — resolved the usual way.</li>
  <li><strong>Blocking files</strong>files that don't conflict but can't be applied: local-changes-overwritten, untracked-overwritten, "already exists" / "could not restore untracked". A counter (<em>"N/total confirmed"</em>) tracks how many have a decision.</li>
</ul>
<p>Each blocking file gets a per-row decision: keep your workdir copy, replace with the stash version, or skip. Identical-bytes paths are filtered out automatically (silent apply), so only real blockers reach the user.</p>

<h2>Keyboard shortcut</h2>
<p>Press <kbd>Esc</kbd> inside the modal to trigger the abort confirmation without losing keyboard focus.</p>

<style>
  .conflict-panels {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 4px;
    border-radius: var(--radius-md);
    overflow: hidden;
    margin: 12px 0;
    font-size: 11px;
    font-family: var(--font-ui-sans);
    background: var(--bg-elevated);
    padding: 4px;
  }
  .cp-col {
    padding: 10px 12px;
    text-align: center;
    font-weight: 600;
    line-height: 1.6;
    background: var(--bg-base);
    border-radius: var(--radius-md);
  }
  .cp-col small { font-weight: 400; color: var(--text-muted); display: block; }
  .cp-ours   { color: var(--success); border-top: 2px solid color-mix(in srgb, var(--success) 45%, transparent); }
  .cp-theirs { color: var(--accent);  border-top: 2px solid color-mix(in srgb, var(--accent) 45%, transparent); }
  .icon-conflict { color: var(--warning); }
  .icon-ok       { color: var(--success); }
</style>
