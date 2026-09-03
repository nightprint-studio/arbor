<!-- Bennu docs — the project tree: what its marks mean, and creating files and folders from it. -->
<h1>The project tree</h1>
<p class="doc-lead">
  The left rail's file tree, what its icons and dimming are telling you, and everything you can do
  to a file without leaving it.
</p>

<h2>Reading the tree</h2>
<p>
  Folder icons carry role, not just structure. A directory holding a <code>Cargo.toml</code> or a
  <code>pom.xml</code> is where a build target is <strong>declared</strong>, so it is drawn as a
  package rather than a folder; a Java package chain is collapsed into one dotted row
  (<code>it.acme.portal</code>) under its source root; and the source roots themselves are tinted by
  what they hold — main, test, resources, webapp.
</p>
<p>
  Entries that are <strong>hidden</strong> (a leading dot) or <strong>ignored by git</strong> are
  dimmed, ignored ones the most, and are still listed: a stale ignored artifact you cannot see is one
  you cannot explain. Nesting is honoured the way git honours it — a nested
  <code>.gitignore</code>, a <code>!negation</code> and <code>.git/info/exclude</code> all count.
  Build output (<code>target</code>, <code>node_modules</code>) and <code>.git</code> are the
  exception: those are skipped outright, because listing them costs more than the rest of the tree.
</p>
<h2>New files</h2>
<p>
  <strong>New ›</strong> from a right-click, from the Project header's <strong>＋</strong>, or from
  the tool-window menu — the same three entries wherever you open it: a typed file, a plain file,
  and a directory. The file entries scaffold in the chosen directory, open the new file and reveal
  it in the tree; they never overwrite an existing one.
</p>
<p>
  Right-clicking the <strong>empty space</strong> below the last row targets the
  <strong>project root</strong> — that is where you create something beside the
  <code>pom.xml</code> rather than inside whatever folder happens to be selected.
</p>
<p>
  <strong>The templates follow the project.</strong> On a Maven root: a Java
  <strong>class / interface / enum / record / annotation / exception</strong>, with the
  <code>package</code> <strong>inferred</strong> from the directory (following
  <code>src/main/java</code> and friends), a <strong>JSP</strong> or <strong>XML</strong> file with
  the right header, or a plain file. On a Cargo root: an empty file, a <strong>struct</strong>,
  <strong>enum</strong>, <strong>trait</strong>, <strong>module</strong> or <strong>test
  module</strong>.
</p>
<p>
  The two families ask for different things, and the field says which. A Java file is named by the
  <em>type</em> it declares; a Rust file names its own <em>module</em>, in <code>snake_case</code>,
  and the template derives the type from it — <code>atlas_player</code> gives you
  <code>atlas_player.rs</code> holding <code>AtlasPlayer</code>. <strong>Module</strong> is the one
  kind that creates a directory (<code>atlas_player/mod.rs</code>), because <code>foo.rs</code> and
  <code>foo/mod.rs</code> are two different decisions about how the module will grow.
</p>
<h2>New folders and packages</h2>
<p>
  <strong>New › Directory</strong> — from a row's right-click menu, from the header's
  <strong>＋</strong>, from the tool-window menu, or as <strong>New folder or package…</strong> in
  the command palette — creates a directory in the chosen folder, and reveals it.
</p>
<p>
  <strong>The name is a path.</strong> <code>assets/icons</code> creates two folders, one inside
  the other; there is no need to open the dialog once per level. Inside a
  <strong>source root</strong> the entry is called <strong>Package</strong> instead, and a
  <strong>dot</strong> separates as well — <code>it.acme.web</code> is three folders, written the
  way the package itself is written. Everywhere else a dot is just a character in a name, so
  <code>.github</code> and <code>my.config</code> stay one folder each. The line under the field
  shows what will exist before you press <kbd>Enter</kbd>.
</p>
<p>
  Levels that are <strong>already there</strong> are stepped through, not objected to: typing
  <code>src/main/resources</code> where <code>src/main</code> exists creates <code>resources</code>
  and nothing else, and the confirmation says exactly what was created.
</p>

<h2>Renaming a file</h2>
<p>
  <kbd>F2</kbd> on a file in the Project tree — or <strong>Rename…</strong> from its right-click menu.
  It refuses to overwrite an existing file, and a rename that changes only the letter case is a rename
  rather than a collision.
</p>
<p>
  For a language with a <strong>language server</strong> behind it, the rename also fixes the code that
  referred to the file by name: renaming a Rust <code>parser.rs</code> rewrites the <code>mod
  parser;</code> that declares it and every <code>use crate::parser::…</code> that goes through it. The
  dialog says how many files that will be <em>before</em> you commit to it, and if the rename itself
  cannot be performed nothing is changed at all. The edits are applied through the editor, so they are
  one undo step.
</p>
<p>
  Directories are deliberately not renamable this way: for a Rust project that moves a whole module
  path, and offering it here would mean offering half of it.
</p>

<h2>Moving a file</h2>
<p>
  <strong>Drag it onto a folder</strong> in the tree, or use <strong>Move to folder…</strong> from
  its right-click menu when the destination is nowhere near the file on screen. A move is the same
  operation as a rename with a different destination, so it carries the same guarantees: the buffer
  is saved first, an open tab follows the file, the destination refuses to be overwritten, and a
  language server's edits (that <code>mod</code> line, those <code>use</code> paths) are applied
  with it.
</p>
<p>
  <strong>It asks first when the move sets off a refactor</strong>, and only then — dragging an
  image into <code>assets/</code> just moves it. The confirmation names the files that will be
  edited, before anything moves.
</p>
<p>
  Moving a <code>.java</code> file into a different <strong>package</strong> is the one case the
  dialog warns about rather than handles: the file moves, and its <code>package</code> line is left
  as it was. Bennu then flags the mismatch on that line, and <kbd>Alt</kbd> + <kbd>Enter</kbd> sets
  it to the new package — but whatever imports the class still names the old one, so a package move
  is not yet a one-gesture refactor.
</p>
<h2>Project tree</h2>
<p>
  The tree <strong>follows the disk</strong>. A <code>git checkout</code>, a <code>cargo new</code>,
  an <code>npm install</code> or another editor saving a file all show up on their own — there is
  nothing to press and nothing to reopen. Changes arrive in bursts, so a checkout touching four
  hundred files is one reload rather than four hundred.
</p>
<p>
  Generated directories are deliberately not watched at all — <code>target</code>,
  <code>node_modules</code>, <code>.git</code>, <code>.svelte-kit</code>, <code>coverage</code> and
  their kind. Not filtered afterwards: <em>unwatched</em>. A build writing into
  <code>target</code> would otherwise be a burst that never goes quiet, which would mean the tree
  refusing to settle at exactly the moment the machine is busiest.
</p>
<p>
  The <strong>Project</strong> panel header carries quick actions: a <strong>New</strong> menu,
  locate the open file in the tree, collapse or expand the whole tree, and an options menu. Right-clicking a file or
  folder opens a context menu (New · Open · Rename · Delete · Local History · Copy path · Reveal in
  Project · Reveal in File Explorer). <kbd>Shift</kbd>+<kbd>F10</kbd> — or the Menu key — opens the
  same menu on the focused row, so every entry in it is reachable without the mouse.
</p>
<p>
  The tree can also be sent somewhere from <em>outside</em>: <strong>Focus in Project</strong>, on a
  crate in the Cargo panel or a module in Dependencies, opens the tree on that folder — expanded,
  selected, and with the keyboard focus on it, so the arrows carry on from there. Clicking any row,
  or opening any file, hands the selection back to whatever the editor is showing.
</p>
<p>
  <strong>Reveal in File Explorer</strong> shows the row on disk: a folder as the listing, a file
  selected inside its folder. It honours <em>Settings → File Explorer → Open in the built-in
  explorer</em>, so it opens Arbor's own explorer window when that is on and the system file
  manager when it is not.
</p>
<p>
  <strong>Delete</strong> — <kbd>Del</kbd> or <kbd>⌫</kbd> on the focused row, or from the context
  menu — asks first, and says what it is about to remove. It does not go to the system trash: the
  files go into Bennu's own <em>local history</em>, which is why <kbd>Ctrl</kbd>/<kbd>Cmd</kbd>+<kbd>Z</kbd>
  with the tree focused puts them back, and why the same undo works identically on every platform.
  The toast that follows carries the same Undo, and days later they are still in <strong>Local
  History › Deleted</strong>.
</p>
<p>
  Two undo stacks, deliberately: <kbd>Ctrl</kbd>/<kbd>Cmd</kbd>+<kbd>Z</kbd> in the editor means
  "un-type that", and in the tree it means "un-delete that". Whichever has focus answers.
</p>
<p>
  Inside a <strong>source root</strong> — <code>src/main/java</code>, <code>src/test/java</code> and
  the matching <code>resources</code> — directories are shown as <strong>packages</strong>: a chain
  with nothing in it but the next directory collapses into one dotted row,
  <code>it.comune.gestionale_atti</code>, with a package icon rather than a folder. The
  three levels of indentation it replaces were spelling one name. A folder that holds files, or more
  than one subfolder, ends the chain and keeps its own row.
</p>
<p>
  Everywhere else the tree stays a plain folder tree — including <code>src/main/webapp</code>, whose
  directories are paths and not packages, because they are what a URL is made of.
</p>
