<script lang="ts">
  /** Picus docs — the DML generator. */
</script>

<h1>Generating DML</h1>
<p class="doc-lead">
  Describe a datum once; Picus writes it into every folder that expects it, in the form
  that folder's engine requires. Generation is deterministic — structured input becomes a
  model, the model is emitted per dialect. No language model is involved at any point.
</p>

<h2>Three sources, one model</h2>
<ul>
  <li><b>Guided form</b> — one row per column, with the types read from the schema.
    Values are validated as you type, not when you save.</li>
  <li><b>Paste SQL</b> — paste statements you already have; they are re-read (parsed, not
    string-substituted) and become the same model. This is the "I have the INSERT, write
    me the other engine's version" case.</li>
  <li><b>CSV</b> — delimiter sniffed, first row as header, and an explicit header →
    column mapping with a same-name proposal. Rows that fail their column types are shown
    as rejected rather than silently dropped.</li>
</ul>
<p>
  Once the rows exist, the source no longer matters: everything downstream sees the same
  <i>table, operation, comparison key, rows</i>.
</p>

<h2>The comparison key</h2>
<p>
  The key decides the <code>WHERE</code> of updates and the existence check behind "skip
  rows that are already there". Leave it unset and it falls back to the primary key —
  the form says so explicitly rather than assuming.
</p>

<h2>Destinations and their rules</h2>
<p>
  Each destination is one file, with its own dialect and its own rules. <b>Add</b> — on the
  Destinations card or in the sidebar — lists every folder that holds scripts, so a
  destination arrives with that folder's engine and its role's preset already applied,
  inherited or declared alike; it can also create a file that does not exist yet, which is
  what a new update script always is. Expanding a destination shows every rule with,
  beside it, what it becomes in the emitted SQL.
</p>
<p>
  A folder <b>with no engine cannot be a destination</b>: there is no form to write the
  statements in. Such a folder is still listed, saying so, with the one action that fixes
  it — see <i>Scripts on disk</i> for how a folder is classified.
</p>
<ul>
  <li><b>Procedural block</b> — <code>DECLARE … BEGIN … END; /</code> on Oracle,
    <code>DO $$ … END $$;</code> on PostgreSQL.</li>
  <li><b>Version guard</b> — run only when the database is at a given version, then carry
    it to the next one. This <b>requires</b> the procedural block: switching the guard on
    switches the block on, and switching the block off drops the guard. Which table holds
    that version, which column carries it, and whether a date is stamped at all are project
    settings (Settings ▸ Version table) — plenty of version tables hold nothing but the
    version string, and Picus then leaves the date out of the UPDATE rather than inventing
    a column.</li>
  <li><b>Skip existing rows</b> — an existence check on the comparison key.</li>
  <li><b>Require the object</b> — <code>USER_TABLES</code> on Oracle,
    <code>to_regclass</code> on PostgreSQL.</li>
  <li><b>Savepoint and rollback</b> — Oracle only; a PostgreSQL <code>DO</code> block is
    already one transaction.</li>
</ul>
<p>
  "Copy these rules" propagates only to destinations with the <b>same role</b>. An
  initialisation script inheriting an update script's version guard would be nonsense, so
  it cannot happen.
</p>
<p>
  When a destination ends up with rules that contradict each other — a version guard with no
  procedural block to return from — it says so, on its own row and above the preview. The
  guard is never quietly dropped: a destination that looks guarded while running
  unconditionally is the one failure you would not notice.
</p>

<h2>What the dialects disagree about</h2>
<p>
  The generator handles, at minimum: block delimiter, upsert syntax
  (<code>MERGE … USING … FROM DUAL</code> against
  <code>INSERT … ON CONFLICT … DO UPDATE</code>), current-date function
  (<code>SYSDATE</code> against <code>CURRENT_TIMESTAMP</code>), object-existence check,
  transaction handling, identifier casing and type mapping.
</p>

<h2>Preview and writing</h2>
<p>
  There are two previews, and they answer different questions. <b>Generated SQL</b> is what
  each destination's rules produce; it regenerates as you change values or rules, with no
  refresh button, because a stale preview is worse than none, and dims for the moment it is
  behind what you have just typed. <kbd>Alt</kbd>+<kbd>←</kbd> and
  <kbd>Alt</kbd>+<kbd>→</kbd> step between destinations.
</p>
<p>
  <b>Changes to the scripts</b> is what each <i>file</i> would look like afterwards. It
  reads the destinations from disk, so it is asked for rather than computed continuously —
  the bottom panel's Changes tab builds it when you look at it, and the write action builds
  it before anything else. What it shows is the exact bytes that would land, produced by
  the same code that performs the write: a diff per file, with the insertion rule stated in
  words, the file's encoding and line ending beside it, and a marker for a file that would
  be created rather than edited. A destination the change would leave untouched says so
  instead of showing an empty diff.
</p>

<h2>Nothing is written that was not reviewed</h2>
<p>
  The confirmation names exactly the files the preview says would change, and writing hands
  the preview's own fingerprints back to the backend. If any of those files moved on
  disk in between — a colleague's pull, an editor still holding the file — the write is
  <b>refused, naming the file that changed</b>, and nothing at all is written. That message
  stays on screen in the Changes tab next to the button that reads the files again, because
  it is the part that tells you what to do next.
</p>
<p>
  When the write does go through it is <b>transactional across all the files</b>: if the
  tenth fails, the first nine are restored from the backup taken beforehand. Encoding and
  line endings are preserved throughout — a windows-1252 file stays windows-1252, a CRLF
  file stays CRLF.
</p>
