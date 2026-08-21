<!-- Bennu docs — the project tree: what its marks mean, and creating files from it. -->
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
  The Project tree's <strong>＋</strong> button — or <strong>New file…</strong> from a right-click
  (or the tool-window menu) — scaffolds a file in the chosen directory. It opens the new file and
  reveals it in the tree; it never overwrites an existing one.
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
  The <strong>Project</strong> panel header carries quick actions: create a new file, locate the open
  file in the tree, collapse or expand the whole tree, and an options menu. Right-clicking a file or
  folder opens a context menu (Open · Rename · Delete · Local History · Copy path · Reveal in
  Project · Reveal in File Explorer). <kbd>Shift</kbd>+<kbd>F10</kbd> — or the Menu key — opens the
  same menu on the focused row, so every entry in it is reachable without the mouse.
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
