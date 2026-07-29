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
<p>
  Picked in the <b>Source</b> card's own header, or with <b>Alt+1</b>, <b>Alt+2</b> and
  <b>Alt+3</b> from anywhere.
</p>
<ul>
  <li><b>Guided form</b> — one field per column, with the types read from the schema.
    Values are validated as you type, not when you save. It composes <b>as many rows as
    you like</b> and shows one at a time: the strip above the fields walks them, duplicates
    the current one — the fast way to enter twenty near-identical parameters — and marks
    the row holding a value that cannot be written, which is the only thing that would
    otherwise tell you the problem is not on the row you are looking at. A row nothing was
    typed into is skipped.</li>
  <li><b>Paste SQL</b> — paste statements you already have; they are re-read (parsed, not
    string-substituted) and become the same model. This is the "I have the INSERT, write
    me the other engine's version" case. The <b>table and the columns come out of the
    statements</b>, so there is no table to pick and this source works with no database
    connected at all. The text is edited in a real SQL editor: highlighting, and — when a
    connection is open — completion over that database's tables and columns.</li>
  <li><b>CSV</b> — delimiter sniffed, first row as header, and an explicit header →
    column mapping with a same-name proposal. Rows that fail their column types are shown
    as rejected rather than silently dropped.</li>
</ul>
<p>
  Once the rows exist, the source no longer matters: everything downstream sees the same
  <i>table, operation, comparison key, rows</i>.
</p>

<h2>Which table, and where its columns come from</h2>
<p>
  The table list holds what the <b>connected database</b> has and what the <b>repository's
  own index</b> knows — a table only the scripts install is a perfectly good destination for
  a row, and requiring a live connection to name one would make the generator unusable on a
  machine with no database at hand.
</p>
<p>
  With a connection, the columns are the server's: real types, real length limits, the
  primary key. Without one — or for a table the connected database does not have — they come
  from <b>SQL</b>: the statements you pasted, or, for a table only the scripts install, what
  the repository's own <code>INSERT</code>s and <code>UPDATE</code>s write into it. So the
  form offers the six columns the scripts actually seed rather than the table's full forty,
  in the order they are written.
</p>
<p>
  Those types are not a guess about the schema — they record how each value was
  <i>written</i>, so a bare number is emitted bare and a quoted one is re-quoted, and a
  column written both ways is quoted, because that is the safe direction. What is lost is the
  checking: no length limits, no <code>NOT NULL</code>, and no primary key to fall back on
  for the comparison key.
</p>
<p>
  A table nothing has to say about — no connection has it and nothing in the repository
  writes to it — says exactly that, rather than showing an empty form.
</p>

<h2>The comparison key</h2>
<p>
  The key decides the <code>WHERE</code> of updates and the existence check behind "skip
  rows that are already there". With a live schema it falls back to the primary key — the
  form says so explicitly rather than assuming. Without one there is no primary key to fall
  back to, so an update, a delete or an upsert asks for it, and Generate stays unavailable
  until it has one: a statement whose <code>WHERE</code> is empty touches every row.
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

<h3>Saved sets</h3>
<p>
  The same change goes to the same four or six places every time, so that list can be saved
  under a name and armed again with one click. The saved sets are the first section of the
  <b>Generate DML</b> sidebar panel; clicking one arms it, and the set matching what is
  currently armed is highlighted. Saving is offered wherever the destinations are —
  the panel's header, <b>Save as…</b> on the Destinations card, or the command palette,
  which also addresses each set by name. Saved with the repository, so a colleague opening
  the same folder finds it.
</p>
<p>
  A set stores <b>folders, not paths</b> wherever it can, and that is what makes it worth
  having: an update destination's file is different every release, so pinning
  <code>4_13.sql</code> would mean writing into a shipped script next month. Applying a set
  asks the repository what this release's update file is called, using the same naming scheme
  the tree uses — and gets the version guard's <b>from</b> and <b>to</b> out of the same
  answer, so a release template arrives complete. The bounds are deliberately never stored:
  last release's numbers filled in automatically would look right and be wrong.
</p>
<p>
  <b>Wherever it can</b> is the qualification that matters. An update folder whose file names
  the scheme cannot read has no "next file" to work out, so the entry keeps the file name it
  was given <i>and</i> the version guard's bounds — the set still works, it just writes into
  that same file every release, and it says so when it is saved and on its own row. The rule
  is one sentence: <b>the entry stores what cannot be derived, and nothing else</b>. The
  alternative is what it sounds like: a destination with no file and an empty guard, which
  quietly disappears. Giving the folder its own naming pattern in <i>Project settings</i> is
  the way to get the release-following behaviour back.
</p>
<p>
  To <b>overwrite</b> a set, save under its name: the <b>Replace</b> button on its sidebar row
  opens the dialog on that name, and the dialog itself lists the existing sets to pick from.
  The confirm button reads <i>Replace</i> rather than <i>Save</i> when the name is taken, so
  overwriting is never something that happens without being read first.
</p>
<p>
  Applying <b>replaces</b> the destinations rather than adding to them, because a set is a
  statement about where this kind of change goes and merging would leave the previous
  release's script armed in a list nobody chose. An entry whose folder has since been
  renamed is skipped and named; the rest of the set still applies.
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
  <li><b>Version row</b> — which row of the version table this destination reads and stamps,
    for a repository that installs several products into one table. Filled in from the
    destination folder's declared product, and editable here for a one-off. Cleared, it
    falls back to the project's own predicate. Shown only under the version guard, because
    outside it nothing reads or stamps a version.</li>
  <li><b>Skip existing rows</b> — an existence check on the comparison key.</li>
  <li><b>Require the object</b> — <code>USER_TABLES</code> on Oracle,
    <code>to_regclass</code> on PostgreSQL.</li>
  <li><b>Savepoint and rollback</b> — Oracle only; a PostgreSQL <code>DO</code> block is
    already one transaction.</li>
</ul>
<p>
  "Copy these rules" propagates only to destinations with the <b>same role</b>. An
  initialisation script inheriting an update script's version guard would be nonsense, so
  it cannot happen. The version row is never copied either: it says which product the
  destination belongs to, and two update scripts of two different products are exactly the
  case it exists for.
</p>

<h2>More than one product in one version table</h2>
<p>
  Some repositories install several products against the same database and keep a version
  per product — one row each, told apart by a column. Which row a generated block should
  read and stamp is then a property of <b>where the script is going</b>, and nothing in the
  SQL says it. So the repository does, in two halves:
</p>
<ul>
  <li><b>Settings ▸ Version table ▸ Products</b> declares what a product <i>is</i>: a name
    and the predicate that selects its row.</li>
  <li>The <b>folder classifier</b> (<kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>F</kbd>) declares
    where its scripts <i>are</i>. It inherits, so naming the product once at the top of
    <code>PORTALE/</code> answers for every version folder underneath, including the ones
    created next month.</li>
</ul>
<p>
  A destination added from such a folder arrives with the right predicate already on it. A
  repository that installs one product declares none of this and behaves exactly as it
  always did.
</p>
<p>
  When a destination ends up with rules that contradict each other — a version guard with no
  procedural block to return from — it says so, on its own row and above the preview. The
  guard is never quietly dropped: a destination that looks guarded while running
  unconditionally is the one failure you would not notice.
</p>

<h2>What the scripts already say</h2>
<p>
  Before writing, Picus reads. Two things it finds change what it writes, and both
  exist because appending blindly is wrong in a repository that has been maintained
  for years.
</p>

<h3>An upsert into an initialisation is a plain insert</h3>
<p>
  "Insert it if it is missing, update it if it is there" is a question about <b>install
  time</b> — and an initialisation runs once, against an empty database, where the answer is
  always <i>missing</i>. The question actually being asked is about <b>authoring</b> time: is
  this row already in the initialisation? Picus answers that by reading the scripts, so the
  destination gets a plain <code>INSERT</code>, or the row already there is changed.
</p>
<p>
  Which also removes a refusal that read as a limitation and was a category error: an upsert
  has no portable spelling, so a <b>portable</b> initialisation could not take one — while
  the thing actually wanted is as portable as SQL gets. An update script still means it: the
  database it runs against is not empty.
</p>

<h3>A row that is already installed</h3>
<p>
  A row is "already there" when <b>any script of that destination's engine</b> installs it —
  the initialisation and the updates together, which is the case that actually occurs: the
  row is in the initialisation and what you are writing is this release's update script. A
  <b>portable</b> destination runs on both engines, so it asks both. Rows are matched on the
  <b>comparison key</b>, because the values are precisely what is changing.
</p>
<p>
  When more than one script installs the same row, the copy that gets changed is the one in
  <b>the file being written to</b>. Only when none of them is that file does the change land
  elsewhere — and then that file joins the diff with its own reason.
</p>
<ul>
  <li>An <b>update</b> script states the row again as <code>DELETE</code> by key then
    <code>INSERT</code>. It never edits history, and the pair lands the same values on a
    database that has the row and on one that does not. The delete matches on the key alone:
    matching every column would leave a hand-edited row in place and then insert a second
    copy of it.</li>
  <li>An <b>initialisation</b> describes the end state, so the row already in it is
    <b>changed where it is</b> — even when that is a different file, which then joins the
    diff with its own reason. Adding a second copy would install a duplicate key.</li>
</ul>
<p>
  <b>The diff says which of these happened.</b> A block appended because the row is genuinely
  new and one appended because nothing could be matched look identical in a file, so the hunk
  header states the judgement: nothing inserts into this table, or these statements do but
  none names one of the key columns — which is the case worth knowing about, because it means
  a key that includes a column the older rows predate.
</p>
<p>
  The scripts are read in <b>install order</b> — the initialisation, then the data, then the
  updates — so a <code>DELETE</code> is seen after the <code>INSERT</code> it removes.
  Only an <b>unconditional</b> <code>DELETE</code> (or a <code>TRUNCATE</code>) makes Picus
  forget the table: "every row is gone" is readable, while "these rows are gone" means
  evaluating a predicate, so a <code>DELETE … WHERE</code> is taken to remove nothing from
  what is remembered. Erring that way is deliberate — remembering a row too eagerly means
  changing an <code>INSERT</code> a later delete removes anyway, while forgetting one means
  appending a second <code>INSERT</code> with the same key.
</p>
<p>
  Otherwise it does nothing clever. A row whose cells are not all literals
  (<code>SYSDATE</code>, a sequence) is not matched, and a row sharing one multi-row
  <code>VALUES</code> statement with others is found but never rewritten — and said so.
</p>

<h3>A block that already guards the same versions</h3>
<p>
  If the destination already has a block guarding <b>4.12 → 4.13</b>, the statements go
  <b>inside it</b>, above the <code>UPDATE</code> that carries the version forward — not into
  a second block on the same range, which would run twice on a fresh install and, on an
  upgraded database, find the version already moved and do nothing.
</p>
<p>
  What is spliced in carries its own marker, so regenerating replaces it in place rather than
  adding a copy on every run. The surrounding guard is yours and is never claimed or rewritten.
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
