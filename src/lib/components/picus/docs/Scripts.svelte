<script lang="ts">
  /** Picus docs — Scripts and encoding. */
</script>

<h1>Scripts on disk</h1>
<p class="doc-lead">
  A script repository is a folder tree with one branch per dialect. Each branch holds
  folders with a <b>role</b> — initialisation, updates, routines, data — and that role
  decides how generated SQL is written into them.
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
  A repository that has never been described to Picus is read by inference: branch per
  dialect from the folder names, role per folder, the dominant encoding per folder, the
  shape of the update filenames. That reading is <b>stated, not assumed</b> — the Scripts
  panel lists what was inferred under the tree, and anything the reader could not settle
  (a folder whose role is unclear, a branch whose dialect is a guess) is listed
  <i>above</i> it, under <b>Needs an answer</b>, because it changes what every row below
  means. Until the repository carries its own description, a banner says so: the layout is
  Picus's reading of the folder, and nothing has been written into it.
</p>

<h2>Roles</h2>
<ul>
  <li><b>Initialisation</b> — runs on a fresh install. Bare statements, no guards: there
    is no earlier version to protect against.</li>
  <li><b>Update</b> — runs on an existing database. Procedural block, guarded on the
    starting version and closing on the resulting one.</li>
  <li><b>Routines</b> — packages, procedures, functions, triggers.</li>
  <li><b>Data</b> — reference rows loaded alongside the schema.</li>
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
  procedure — which statements touch it, in which file, in which branch, under which
  role. The Inventory tab shows that as a matrix, and a zero in any column is a branch
  staying silent about something the other one says.
</p>
<p>
  Anything indexed that belongs to no branch is listed separately, under <b>Outside every
  branch</b>. It is not a gap between the two dialects — it is a place outside the model
  altogether, and leaving it out would make the matrix look complete when it is not.
</p>
