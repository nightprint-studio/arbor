<script lang="ts">
  /** Picus docs — Scripts and encoding. */
</script>

<h1>Scripts on disk</h1>
<p class="doc-lead">
  A script repository is a folder tree, and Picus shows it as it is. Any folder can say
  which <b>engine</b> its scripts are written for and what they are <b>for</b> — their
  role: initialisation, updates, routines, data. Everything beneath a folder inherits both
  until another folder says otherwise.
</p>

<h2>The tree is the tree</h2>
<p>
  Repositories do not agree on where the engine goes. Some keep an <code>ORACLE/</code>
  and a <code>POSTGRES/</code> at the top with the roles inside. Others deliver one folder
  per released version and put the engine at the very bottom —
  <code>AGGIORNAMENTO/4.13.2/ORA</code>, <code>…/POS</code>, <code>…/MSQ</code>. Both are
  described by the same rule, so neither has to be reshaped to be understood, and the
  panel never invents a level that is not on disk.
</p>
<p>
  Deep trees stay readable: indent guides carry the level, every row's full path is in its
  tooltip, and a folder whose name repeats elsewhere in the repository — the eleventh
  <code>ORA</code> — is prefixed with the folder above it, which is exactly the part that
  tells them apart. Nothing is prefixed when the name is already unique.
</p>

<h2>Declared here, or inherited</h2>
<p>
  The engine and the role each appear as a chip on the folder row, and the two states look
  different on purpose: a <b>solid</b> chip is set on that folder, a <b>quiet</b> one is
  inherited from a folder above. The tooltip names which one. Without that distinction
  there is no way to tell whether a row is where a wrong answer should be corrected, or
  merely where it is felt.
</p>

<h2>Four answers about an engine</h2>
<p>
  A folder is not simply "a Picus engine or a question". There are four states, and each
  behaves differently:
</p>
<ul>
  <li><b>Oracle or PostgreSQL</b> — Picus reads these. Their scripts are parsed, indexed,
    compared across engines, and generated into.</li>
  <li><b>Portable</b> — plain SQL written to run on <i>both</i> engines. The chip says
    <b>portable · both engines</b>. See below: this one changes what several other things
    mean.</li>
  <li><b>An engine Picus does not support</b> — SQL Server, DB2, MySQL, MariaDB, SQLite. The
    chip names the engine and says <i>not supported</i>. Those scripts are listed and left
    alone: never parsed, never compared, never written into, and <b>never asked about
    again</b>. Parsing T-SQL with a grammar built for Oracle and PostgreSQL does not fail —
    it produces plausible-looking nonsense, which is worse than nothing.</li>
  <li><b>No engine</b> — nobody has said yet. The muted <code>?</code>, and the only one of
    the four that is a question you are expected to answer.</li>
</ul>
<p>
  The distinction matters because the last three used to be one. A repository whose
  <code>MSQ</code> folders are SQL Server carried a permanent row of warnings about something
  nobody could ever fix; a folder of portable inserts had to be declared Oracle or PostgreSQL,
  which was untrue either way and made the engine it was <i>not</i> look like it was missing
  everything the folder contained.
</p>

<h2>Portable folders</h2>
<p>
  Some folders hold plain <code>INSERT</code> / <code>UPDATE</code> / <code>DELETE</code> that
  is valid on Oracle and on PostgreSQL alike. Declaring one <b>portable</b> says exactly that,
  and three things follow:
</p>
<ul>
  <li><b>It counts for every engine.</b> A row inserted by a portable script is present on
    both, so neither is ever reported as missing what the folder contains. It takes part in
    both engines' comparisons, and it is the only kind of folder that does.</li>
  <li><b>Anything belonging to one engine is a finding.</b> <code>MERGE … FROM DUAL</code>,
    <code>ON CONFLICT</code>, <code>SYSDATE</code>, <code>now()</code>, <code>CONNECT BY</code>,
    <code>$$</code> — each of them keeps the promise on one engine and breaks it on the other,
    so in a portable folder each of them is reported. That is a stricter check than the one it
    replaces, and the message says what the folder promised rather than which engine the
    construct belongs to.</li>
  <li><b>Generation is allowed, restricted to what both accept.</b> One file instead of two,
    which is the payoff. Plain statements only: <b>no procedural block</b> (Oracle spells it
    <code>DECLARE … BEGIN … END; /</code> and PostgreSQL <code>DO $$ … $$</code>, and no form
    runs on both), and therefore <b>no version guard</b> — it needs the block to return from.
    No upsert either, for the same reason as above. <code>CURRENT_TIMESTAMP</code> is what
    "now" becomes, because it is standard and both engines take it. Anything Picus cannot
    write portably is refused with the reason, on the destination it belongs to.</li>
</ul>
<p>
  It is <b>never inferred</b>. No folder name produces it: a promise that these scripts run on
  both engines is one you make, not one a name implies. Pick it from the Engine list, or
  declare a name portable in the project's vocabulary.
</p>

<h2>Saying what a folder is</h2>
<p>
  Right-click a folder in the tree — or press <kbd>Shift</kbd>+<kbd>F10</kbd> on the
  focused row — and set its <b>Engine</b> and its <b>Role</b>. The engine list offers the two
  Picus reads first, then <b>Portable</b>, then the ones it can only name. <b>Inherit from above</b>
  in either submenu clears the folder's own declaration so it follows its ancestors again;
  it is greyed out when the folder never declared one, because there would be nothing to
  clear.
</p>
<p>
  <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>F</kbd> opens the same thing as a dialog: type
  part of a path, walk the matches with <kbd>↑</kbd> <kbd>↓</kbd>, pick the two answers
  and press <kbd>Ctrl</kbd>+<kbd>Enter</kbd>. The command palette lists it, and lists each
  folder of scripts that still has no engine by name.
</p>
<p>
  Applying writes the repository's own configuration —
  <code>.arbor/picus/project.toml</code>, inside the repository, alongside the scripts —
  and Picus says where. That is deliberate: a colleague opening the same folder must read
  it the same way, or the same repository behaves differently per person, which is the
  class of surprise Picus exists to remove. <b>Nothing is written before that</b>.
</p>

<h2>Saying it once, for every folder of that name</h2>
<p>
  Repositories that ship a folder set per delivered version have the same folder names over
  and over: eleven called <code>ORA</code>, eleven called <code>POS</code>, and another set
  next release. Classifying them one by one is eleven decisions that will be twelve next
  month, so Picus lets a repository declare what a <b>name</b> means:
</p>
<pre><code>{`[[alias]]
name = "POS"
engine = "postgres"

[[alias]]
name = "MSQ"
engine = "sqlserver"

[[alias]]
name = "COMUNE"
engine = "generic"

[[alias]]
name = "CONSEGNE"
role = "update"`}</code></pre>
<p>
  Picus's own vocabulary only holds names that mean the same thing in every repository —
  <code>ORACLE</code>, <code>ORA</code>, <code>POSTGRES</code>, <code>PG</code>,
  <code>MSSQL</code>, <code>DB2</code>. It deliberately does not know <code>POS</code>: that
  is PostgreSQL in your repository and <code>POSIZIONI</code> in somebody else's, and a
  three-letter guess that general would quietly misclassify a folder. Your repository knows,
  so your repository says.
</p>
<ul>
  <li>Names are matched <b>whole word, case-insensitively</b> — <code>POS</code> matches
    <code>POS</code>, <code>01_POS</code> and <code>POS_2024</code>, and never
    <code>POSIZIONI</code>.</li>
  <li>A name can carry an <b>engine</b>, a <b>role</b>, or both — including an engine Picus
    does not support, and including <code>generic</code> for folders of portable SQL.</li>
  <li>It <b>adds to</b> Picus's own vocabulary rather than replacing it — declaring one name
    never costs you the defaults.</li>
  <li>It applies as the repository is read, so a folder of that name added <b>later</b> is
    classified without touching anything.</li>
  <li>A folder that declares its own engine keeps it: a specific answer beats the rule.</li>
  <li>A value Picus does not recognise costs that one name and nothing else — the rest of
    the configuration still loads, and the panel says what was wrong with it.</li>
</ul>
<p>
  Classify a folder whose name repeats and Picus offers, as a <b>separate</b> question, to
  make it a rule — naming how many folders that would reach before you agree. Declining
  leaves the folder you just classified exactly as you set it. The names a project has
  accumulated are listed, editable and removable under
  <b>Settings ▸ Project ▸ Folder names</b>, each showing how many folders it currently
  reaches; the command palette opens it by name.
</p>

<h2>A repository belongs to a connection</h2>
<p>
  Picus is database-oriented rather than project-oriented: a repository is <i>the folder
  this database is installed from</i>, so it is attached to a connection and remembered
  with it. Open that connection and its scripts, its inventory and its consistency report
  are what the window shows; switch connection and everything follows.
</p>
<p>
  Attach one from the connection editor (Scripts ▸ <b>Choose…</b>), from the <b>Scripts on
  disk</b> panel, or from the command palette by connection name. Detaching only stops
  Picus from showing the folder — nothing on disk is touched. A connection used purely for
  queries never needs one.
</p>
<p>
  <kbd>F5</kbd> re-reads the repository from disk: files change under the tool constantly
  — a colleague's pull, an external editor — and the tree is a snapshot until asked
  otherwise.
</p>

<h2>What Picus infers, and what it asks</h2>
<p>
  A repository that has never been described to Picus is read by inference: the engine and
  the role guessed from folder names, the dominant encoding per folder, the shape of the
  update filenames. That reading is <b>stated, not assumed</b> — the Scripts panel lists
  what was inferred under the tree, and anything the reader could not settle (a folder
  whose role is unclear, one whose engine is a guess) is listed <i>above</i> it, under
  <b>Needs an answer</b>, because it changes what every row below means. Until the
  repository carries its own description, a banner says so: the layout is Picus's reading
  of the folder, and nothing has been written into it.
</p>
<p>
  Folders holding scripts that no engine covers get their own warning, because they are
  the ones that stop the repository working: nothing is generated into them and nothing
  about them is compared. Classifying one — or any folder above it, or its name — is what
  fixes it. Folders in an engine Picus does not support are counted separately and stated
  rather than warned about: there is nothing to fix there.
</p>

<h2>Roles</h2>
<ul>
  <li><b>Initialisation</b> — runs on a fresh install. Bare statements, no guards: there
    is no earlier version to protect against.</li>
  <li><b>Update</b> — runs on an existing database. Procedural block, guarded on the
    starting version and closing on the resulting one.</li>
  <li><b>Routines</b> — packages, procedures, functions, triggers.</li>
  <li><b>Data</b> — reference rows loaded alongside the schema.</li>
  <li><b>Ignored</b> — not indexed and never written into. A real choice, for the folder
    of one-off fixes that should take no part in any of this.</li>
</ul>

<h2>Encoding</h2>
<p>
  Legacy repositories are usually windows-1252, and one save from the wrong editor
  silently converts a file to UTF-8 and breaks every accented character in it. The project
  <b>declares</b> its encoding (Settings ▸ Project), and detection is compared against that
  declaration — which is what turns "this file is UTF-8" from a fact into a finding. Picus
  treats encoding as first-class throughout:
</p>
<ul>
  <li>It is detected <b>per file</b>: a byte-order mark decides outright; otherwise valid
    UTF-8 with at least one multibyte sequence means UTF-8; a pure-ASCII file is ambiguous
    and inherits the folder's dominant encoding, marked as such.</li>
  <li>It is <b>shown</b> — on the file row, in the editor's bar and in the status bar,
    alongside the line ending.</li>
  <li>It is <b>preserved</b>: a file read as windows-1252 is written back as
    windows-1252, and a CRLF file stays CRLF.</li>
  <li>A character that cannot be represented in the destination encoding <b>blocks the
    write</b> and says which character and which line. Never a silent replacement, never
    a <code>?</code>.</li>
</ul>
<p>
  When a file comes back from disk in a different encoding than the one expected, its
  badge turns red and the consistency report raises <code>ENC001</code> with the list of
  characters that changed.
</p>

<h2>Inventory</h2>
<p>
  Indexing the repository builds an inventory: for every object — table, view, package,
  procedure — which statements touch it, in which file, in which folder. The Inventory tab
  shows that as a matrix whose columns are <b>engine × role</b>, not folders: a repository
  with a folder per released version has hundreds of those, and a column each answers a
  question nobody asks. A zero in a column is one engine staying silent about something
  another one says, or the updates staying silent about something the initialisation says.
</p>
<p>
  The per-folder detail is not lost — expand an object's row and every column breaks down
  into the folders behind it, which is where "<i>which</i> of the eleven version folders is
  missing it" is actually answered. Statements that land in no column at all are counted
  on the row as <b>elsewhere</b>, and files under an <b>ignored</b> folder — or one in an
  engine Picus does not read — are stated under the table, so a folded matrix can never
  look complete when it is not. Those folders get no column of their own: their files are
  never parsed, so the column could only ever read zero, and a permanent row of zeroes
  would say "missing everything" about scripts that are simply none of Picus's business.
</p>
<p>
  Anything indexed that belongs to no classified folder is listed separately, under
  <b>Outside every classified folder</b>. It is not a gap between two engines — it is a
  place outside the model altogether, and leaving it out would make the matrix look
  complete when it is not.
</p>
