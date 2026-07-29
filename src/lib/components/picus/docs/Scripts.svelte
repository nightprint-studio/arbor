<script lang="ts">
  /** Picus docs — Scripts and encoding. */
</script>

<h1>Scripts on disk</h1>
<p class="doc-lead">
  A script repository is a folder tree, and Picus shows it as it is. Any folder can say
  which <b>engine</b> its scripts are written for and what they are <b>for</b> — their
  role: initialisation, updates, routines, data. Everything beneath a folder inherits both
  until another folder says otherwise, and a single <b>file</b> can say which engine it is
  when its folder cannot.
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
  <li>A name means folder names unless you say otherwise. Pointing it at <b>file</b> names too
    is one more line — see <i>When the engine is in the file name</i>, below.</li>
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
  make it a rule — naming how many folders that would reach before you agree. Classify a
  script and it offers the same thing about a word of its name, proposing the word that recurs
  across the most files and letting you correct the guess. Either way the offer asks
  <b>where the name applies</b> — folder names, file names, or both — with the count beside
  each, and it starts on the axis you were working on rather than the wider one. Declining
  leaves what you just classified exactly as you set it.
</p>
<p>
  The names a project has accumulated are listed, editable and removable under
  <b>Settings ▸ Project ▸ Folder names</b>, each showing where it applies and how many folders
  it currently reaches; the command palette opens it by name.
</p>

<h2>When the engine is in the file name</h2>
<p>
  Not every repository puts the engine in a directory. Plenty hold
  <code>4_12_ORA.sql</code> and <code>4_12_POS.sql</code> side by side in one folder, and that
  folder is honestly neither: it is both. So the engine is a property of the <b>file</b>, of which
  the folder is only the default. A file that says nothing is in its folder's engine — which is
  every file in a tidy repository — and a file that says something wins over the folder it sits
  in, the same way a folder wins over the one above it.
</p>
<p>
  Point a name at file names and one line classifies every scattered script:
</p>
<pre><code>{`[[alias]]
name = "POS"
engine = "postgres"
applies_to = "both"      # "folders" (the default), "files", or "both"`}</code></pre>
<p>
  Or answer for one path, when a single file is the exception:
</p>
<pre><code>{`[[file]]
path = "AGGIORNAMENTO/2024/4_12_POS.sql"
dialect = "postgres"`}</code></pre>
<p>
  A file declaration carries the engine and nothing else. A <b>role</b> is what a directory of
  scripts is for, and the script beside this one is for the same thing; an <b>encoding</b> is
  measured from the bytes rather than declared. The engine is the one thing that genuinely varies
  file by file, so it is the one thing this says.
</p>
<p>
  Neither line has to be typed. Right-click a file in the tree — or press
  <kbd>Shift</kbd>+<kbd>F10</kbd> on the focused row — and set <b>Engine of this file</b>; the
  folder's own entry stays right below it, because most of the time the correction really does
  belong to the folder. <kbd>F6</kbd> opens the same thing as a dialog: type part of a path,
  walk the matches with <kbd>↑</kbd> <kbd>↓</kbd>, pick the engine and press
  <kbd>Ctrl</kbd>+<kbd>Enter</kbd>. <b>Inherit from the folder</b> clears the declaration
  again, and the dialog names what the file would fall back to before you do it. The command
  palette lists the dialog, and lists every script that declares an engine of its own by name.
</p>
<p>
  The tree stays quiet about all of this on purpose. A file row carries an engine chip only
  when it says something the folder header does not: when it <b>declares its own engine</b>, or
  when it has <b>no engine while a script beside it has one</b> — the odd one out in a folder
  somebody has started sorting by file name, and the only one nothing is generated into.
  Everything else inherits silently, because a badge on all five hundred rows is the folder's
  badge repeated five hundred times, and a badge that is always there is one nobody reads.
</p>
<p>
  <b>Picus never reads an engine out of a file name on its own</b>, and that is deliberate rather
  than cautious. A folder name is short and chosen; a file name is a sentence.
  <code>ORA</code> is Italian for <i>now</i>, so <code>AGGIORNA_ORA_INIZIO.sql</code> would read as
  Oracle, and <code>MIGRAZIONE_DA_MYSQL.sql</code> is a PostgreSQL script <i>about</i> MySQL —
  reading <code>mysql</code> out of it would not produce a wrong finding, it would produce
  <i>no</i> findings at all, silently. A repository has a dozen folder names and hundreds of file
  names, and nobody reviews hundreds. So a file is classified by its name only where you have said
  which names mean what, <i>and</i> said you meant it about file names. The extension is never part
  of the match.
</p>
<p>
  What follows once files carry engines: a folder holding both takes part in <b>both</b> engines'
  comparisons instead of neither, its files are each parsed as the dialect they actually are, and
  a repository whose PostgreSQL side is four scattered <code>*_POS.sql</code> files genuinely has a
  PostgreSQL side. In the Inventory, such a folder's column <b>splits per engine</b> —
  <code>AGG · Oracle</code>, <code>AGG · PostgreSQL</code> — because one column would add the two
  together and destroy the only comparison the table is there to make. A folder with a single
  engine keeps a single column, headed with its path alone.
</p>
<p>
  When a file disagrees with a folder that <i>declared</i> its engine, Picus says so under the
  tree — not as a question, but as a statement that a specific answer is overruling a general one.
  Nothing else in the folder is affected.
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
  about them is compared. The question is asked where a <b>file</b> is left over, not merely
  where a folder is silent — a directory whose scripts have each answered for themselves is
  settled even though the directory itself could never say what engine it is. Classifying a
  folder, a file, or a name is what fixes it. Folders in an engine Picus does not support are
  counted separately and stated rather than warned about: there is nothing to fix there.
  <b>Excluded</b> folders and scripts raise none of this at all — they are not in the project,
  so nothing about them is a question.
</p>

<h2>Roles</h2>
<ul>
  <li><b>Initialisation</b> — runs on a fresh install. Bare statements, no guards: there
    is no earlier version to protect against.</li>
  <li><b>Update</b> — runs on an existing database. Procedural block, guarded on the
    starting version and closing on the resulting one.</li>
  <li><b>Routines</b> — packages, procedures, functions, triggers.</li>
  <li><b>Data</b> — reference rows loaded alongside the schema.</li>
  <li><b>Ignored</b> — not an installation folder: nothing is generated into it and it
    takes part in no comparison between engines. It is still read, its objects still
    appear in the inventory and its files are still checked. A real choice, for the
    folder of one-off fixes that should take no part in the installation.</li>
</ul>

<h2>Leaving something out of the project</h2>
<p>
  Some scripts are none of Picus's business at all — the migration folder that ran once in
  2019, the export somebody committed by mistake. Those are <b>excluded</b>: Picus treats
  them as though they were not in the repository. Not parsed, not indexed, no coverage
  column, no findings, and never a destination for a generation.
</p>
<p>
  Right-click a folder or a script in the tree — or press <kbd>Shift</kbd>+<kbd>F10</kbd>
  on the focused row — and choose <b>Exclude this folder from the project</b> or
  <b>Exclude this script from the project</b>. The command palette offers the same for the
  script you have open, and lists everything currently excluded so it can be put back by
  name. Excluding a folder covers everything beneath it.
</p>
<pre><code>{`[[folder]]
path = "MIGRAZIONE_2019"
excluded = true

[[file]]
path = "AGGIORNAMENTO/2024/export_una_tantum.sql"
excluded = true`}</code></pre>
<p>
  <b>Excluded is not the <i>ignored</i> role</b>, and the difference is worth the two
  sentences. <i>Ignored</i> says <i>this is not an installation folder</i>: nothing is
  generated into it and it is compared with nothing, but it is still read, its objects still
  show up in the inventory and its files are still checked — knowing that
  <code>MIGRAZIONE_2019</code> creates a table is worth having. <i>Excluded</i> says
  <i>pretend this is not in the repository</i>. The two cannot be one setting, because
  <i>ignored</i> is also what a folder nobody has classified falls back to: if that meant
  excluded, the folders most in need of attention would be the ones silently dropped from
  the report.
</p>
<p>
  One script can be kept out of an excluded folder's fate. A folder of migrations that holds
  the one file that does matter says so on the file, and the tree's menu offers it as
  <b>Keep this script in the project</b>:
</p>
<pre><code>{`[[folder]]
path = "MIGRAZIONE_2019"
excluded = true

[[file]]
path = "MIGRAZIONE_2019/4_12__4_13.sql"
excluded = false`}</code></pre>
<p>
  Excluded rows <b>stay in the tree</b>, dimmed and struck through, with the badge on the
  row that made the decision. Hiding them would leave no way to change your mind. An
  excluded folder starts <b>collapsed</b> — what is inside it is not what the panel is for —
  and opens like any other with <kbd>→</kbd> or a click, which is how the script that needs
  rescuing is reached.
</p>

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
  question nobody asks.
</p>
<p>
  There is <b>one table per kind</b> — tables, views, sequences, each its own — because the
  question people bring here is about one kind at a time. The columns line up across all of
  them, so an Oracle package and a PostgreSQL function are still compared by looking
  straight down. A collapsed legend above the tables says what the numbers and the marks
  mean.
</p>
<p>
  A number is how many statements <b>change</b> the object there, and more than one is
  perfectly normal — a table created once and altered by four update scripts reads 5.
  Nothing is marked for that. What <i>is</i> marked, in red, is a <b>gap</b>: one side
  staying silent about something another side installs. That judgement is exactly the one
  <code>CONS001</code> makes, so what is marked here is what the consistency report raises —
  a dash that is not a gap is left plain. An object nothing here installs, and one covered
  by a portable folder at the same role, are therefore never marked.
</p>
<p>
  The per-folder detail is not lost — expand an object's row and every column breaks down
  into the folders behind it, which is where "<i>which</i> of the eleven version folders is
  missing it" is actually answered. A folder holding more than one engine appears there once per
  engine, with the engine named beside its path, so its Oracle and its PostgreSQL numbers are never
  added together. Statements that land in no column at all are counted
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
