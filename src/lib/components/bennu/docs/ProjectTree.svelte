<script lang="ts">
  /**
   * The project tree: what its icons and dimming say, creating files, modules and folders from it, copying, renaming,
   * moving and deleting — and how it keeps up with the disk.
   */
  import Callout from '$lib/components/shared/ui/Callout.svelte';
  import { highlightCode } from '$lib/utils/highlight';
</script>

<span class="eyebrow">Projects</span>
<h1>The project tree</h1>

<p class="doc-lead">
  The left rail's file tree: what its icons and its dimming are telling you, and everything you can do to a file without
  leaving it.
</p>

<h2>Reading the tree</h2>
<div class="feature-grid">
  <div class="feature-card">
    <div class="fc-eyebrow">A folder holding <code>pom.xml</code> or <code>Cargo.toml</code></div>
    <div class="fc-title">A package icon</div>
    <div class="fc-desc">That is where a build target is <strong>declared</strong>, so it is drawn as a package rather than a folder.</div>
  </div>
  <div class="feature-card">
    <div class="fc-eyebrow">Main, test, resources, webapp</div>
    <div class="fc-title">Tinted source roots</div>
    <div class="fc-desc">Each source root is tinted by what it holds.</div>
  </div>
  <div class="feature-card">
    <div class="fc-eyebrow">A leading dot, or <code>.gitignore</code></div>
    <div class="fc-title">Dimmed rows</div>
    <div class="fc-desc">Hidden entries are dimmed and ignored ones most of all — but still listed: a stale ignored artifact you cannot see is one you cannot explain.</div>
  </div>
</div>
<p>
  Ignoring is honoured the way git honours it: a nested <code>.gitignore</code>, a <code>!negation</code> and
  <code>.git/info/exclude</code> all count. Build output (<code>target</code>, <code>node_modules</code>) and
  <code>.git</code> are the exception — those are skipped outright, because listing them costs more than the rest of the tree.
</p>

<h3>Packages</h3>
<p>
  Inside a <strong>source root</strong> — <code>src/main/java</code>, <code>src/test/java</code> and the matching
  <code>resources</code> — directories are shown as <strong>packages</strong>. A chain with nothing in it but the next
  directory collapses into one dotted row with a package icon:
</p>
<pre><code>src/main/java
  it.comune.gestionale_atti
    AttoService.java</code></pre>
<p>
  The three levels of indentation it replaces were spelling one name. A folder that holds files, or more than one
  subfolder, ends the chain and keeps its own row. Everywhere else the tree stays a plain folder tree — including
  <code>src/main/webapp</code>, whose directories are paths and not packages, because they are what a URL is made of.
</p>

<h2>New files</h2>
<p>
  <strong>New ›</strong> — from a right-click, from the Project header's <strong>＋</strong>, or from the tool-window menu —
  offers the same three entries wherever you open it: a typed file, a plain file, and a directory. A file is created in the
  chosen directory, opened and revealed in the tree; an existing file is never overwritten.
</p>
<Callout variant="tip" title="Creating beside the pom">
  Right-click the <strong>empty space</strong> below the last row: that targets the <strong>project root</strong>, rather
  than whatever folder happens to be selected.
</Callout>
<p><strong>The kinds follow the project:</strong></p>
<div class="feature-grid two-col">
  <div class="feature-card">
    <div class="fc-eyebrow">A Maven project</div>
    <div class="fc-title">Java, JSP, XML</div>
    <div class="fc-desc">
      A <strong>class, interface, enum, record, annotation</strong> or <strong>exception</strong>, with the
      <code>package</code> <strong>inferred</strong> from the directory — following <code>src/main/java</code> and friends —
      a <strong>JSP</strong> or <strong>XML</strong> file with the right header, or a plain file.
    </div>
  </div>
  <div class="feature-card">
    <div class="fc-eyebrow">A Cargo project</div>
    <div class="fc-title">Rust</div>
    <div class="fc-desc">
      An empty file, a <strong>struct</strong>, <strong>enum</strong>, <strong>trait</strong>, <strong>module</strong> or
      <strong>test module</strong>.
    </div>
  </div>
</div>
<p>
  The two ask for different things, and the field says which. A Java file is named after the <em>type</em> it declares. A
  Rust file names its own <em>module</em>, in <code>snake_case</code>, and the type is derived from it:
  <code>atlas_player</code> gives <code>atlas_player.rs</code> holding <code>AtlasPlayer</code>.
</p>
<Callout variant="info" title="Module is the one that makes a folder">
  <strong>Module</strong> creates <code>atlas_player/mod.rs</code>, because <code>foo.rs</code> and <code>foo/mod.rs</code>
  are two different decisions about how the module will grow.
</Callout>
<p>
  Code templates of your own are listed in the same dialog, under <em>Your templates</em> — see <strong>Code templates</strong>.
</p>

<h2>New modules</h2>
<p>
  <strong>New › Module</strong> in the tree's right-click menu, or <strong>New module…</strong> in the command palette. Maven
  projects only.
</p>
<p>
  A module is not a folder: it is a folder <em>its parent knows about</em>. So creating one goes like this:
</p>
<ol class="step-list">
  <li>Choose which pom of the reactor will list it — preselected to the one nearest the row you opened the menu on.</li>
  <li>Name it, and choose its packaging.</li>
  <li>Leave group and version as they are, unless you mean otherwise (see below).</li>
  <li>Create: the folder, the pom and the source roots are written, and the parent gets its <code>&lt;module&gt;</code>
    line. A parent that was not an aggregator becomes one (<code>&lt;packaging&gt;pom&lt;/packaging&gt;</code>) — the
    dialog says so before you press Create.</li>
</ol>
<Callout variant="tip" title="Group and version are placeholders, not values">
  What is greyed out is what the parent gives, and leaving it alone writes nothing into the new pom — the right pom almost
  every time. A module that repeats its parent's version is how a release ends up needing eleven files edited. Type over
  the placeholder to say you meant otherwise.
</Callout>
<table>
  <thead><tr><th>Packaging</th><th>Folders created</th></tr></thead>
  <tbody>
    <tr><td><code>jar</code></td><td><code>src/main/java</code> … <code>src/test/resources</code></td></tr>
    <tr><td><code>war</code></td><td>The same, and <code>src/main/webapp/WEB-INF</code></td></tr>
    <tr><td><code>pom</code></td><td>None — sources in an aggregator would never be compiled</td></tr>
  </tbody>
</table>
<p>
  Nothing is written until every refusal has been checked: an occupied directory, a name that is not a folder name, or a
  parent that builds sources of its own and so cannot also aggregate.
</p>

<h2>New folders and packages</h2>
<p>
  <strong>New › Directory</strong> — from a row's right-click menu, the header's <strong>＋</strong>, the tool-window menu,
  or <strong>New folder or package…</strong> in the command palette — creates a directory in the chosen folder and reveals it.
</p>
<p>
  <strong>The name is a path</strong>, so there is no need to open the dialog once per level. Inside a
  <strong>source root</strong> the entry is called <strong>Package</strong>, and a <strong>dot</strong> separates too — the
  package written the way the package itself is written. Everywhere else a dot is just a character of a name.
</p>
<table>
  <thead><tr><th>Typed</th><th>Where</th><th>Creates</th></tr></thead>
  <tbody>
    <tr><td><code>assets/icons</code></td><td>Anywhere</td><td><code>assets</code>, and <code>icons</code> inside it</td></tr>
    <tr><td><code>it.acme.web</code></td><td>A source root</td><td>Three folders, <code>it/acme/web</code></td></tr>
    <tr><td><code>.github</code>, <code>my.config</code></td><td>Outside a source root</td><td>One folder each</td></tr>
    <tr><td><code>src/main/resources</code></td><td>Where <code>src/main</code> exists</td><td>Only <code>resources</code> — levels already there are stepped through</td></tr>
  </tbody>
</table>
<p>
  The line under the field shows what will exist before you press <kbd>Enter</kbd>, and the confirmation says exactly what
  was created.
</p>
<Callout variant="info" title="A package is a name, not a place">
  In package territory the field opens already filled with the package you were on, every part of it editable, and the
  folders are created from the <strong>source root</strong>. A child is <kbd>Enter</kbd> after one word; a sibling, or a
  package one level up, is a few <kbd>Backspace</kbd>s. Outside a source root the field starts empty and folders are created
  <em>inside</em> the one you picked, because there a chain of directories really is a place.
</Callout>

<h2>Copying a class</h2>
<ol class="step-list">
  <li><kbd>Ctrl</kbd> + <kbd>C</kbd> on a file in the tree — or <strong>Copy</strong> from its right-click menu.</li>
  <li><kbd>Ctrl</kbd> + <kbd>V</kbd> on the package or folder it should land in — or <strong>Paste</strong>.</li>
  <li>The dialog opens with the name selected and the extension left out of the selection: renaming is one word and
    <kbd>Enter</kbd>.</li>
</ol>
<p>
  For a <code>.java</code> file the copy is not a file copy. Copying <code>OrderService</code> from
  <code>it.acme.orders</code> into <code>it.acme.billing</code> as <code>InvoiceService</code>:
</p>
<pre><code>{@html highlightCode(`package it.acme.orders;

public class OrderService {
    private final OrderRepository repository;

    public OrderService(OrderRepository repository) { … }
}`, 'java')}</code></pre>
<pre><code>{@html highlightCode(`package it.acme.billing;

import it.acme.orders.OrderRepository;

public class InvoiceService {
    private final OrderRepository repository;

    public InvoiceService(OrderRepository repository) { … }
}`, 'java')}</code></pre>
<ul>
  <li>The <code>package</code> declaration is rewritten to where the copy lands.</li>
  <li>The type is renamed with the file — its declaration, its constructors and every mention of it as a type. A field that
    happens to share the name is left alone: the rename walks what the parser calls type names, not the text.</li>
  <li>The classes it used by <strong>simple name</strong> — its old neighbours, which needed no import while they shared a
    package — become imports. That is the part that silently breaks when a class is duplicated by hand.</li>
</ul>
<Callout variant="info" title="When both packages have the name">
  If the old and the new package both declare the same simple name, no import is written: the copy resolves to its new
  neighbour, and picking the other one for you would be a guess.
</Callout>
<p>
  Nothing is ever overwritten: a paste that would land on an existing file says so and waits for another name. Folders are
  not pasted — every file inside one would need its own package rewrite and its own collision check, and half of that would
  be worse than none.
</p>

<h2>Renaming a file</h2>
<p>
  <kbd>F2</kbd> on a file in the tree, or <strong>Rename…</strong> from its right-click menu. It refuses to overwrite an
  existing file, and a rename that only changes letter case is a rename, not a collision.
</p>
<p>
  For a language with a <strong>language server</strong> behind it, the rename also fixes the code that names the file.
  Renaming a Rust <code>parser.rs</code> rewrites the <code>mod parser;</code> that declares it and every
  <code>use crate::parser::…</code> that goes through it. The dialog says how many files that touches <em>before</em> you
  commit, if the rename itself cannot be performed nothing changes at all, and the edits go through the editor as one undo step.
</p>
<Callout variant="info" title="Directories are not renamed this way">
  For a Rust project that moves a whole module path, and offering it here would mean offering half of it.
</Callout>

<h2>Moving a file</h2>
<p>
  <strong>Drag it onto a folder</strong> in the tree, or use <strong>Move to folder…</strong> from its right-click menu when
  the destination is nowhere near it on screen. A move is a rename with a different destination, so it has the same
  guarantees: the buffer is saved first, an open tab follows the file, the destination is never overwritten, and a language
  server's edits — that <code>mod</code> line, those <code>use</code> paths — are applied with it.
</p>
<p>
  <strong>It asks first when the move sets off a refactor</strong>, and only then: dragging an image into
  <code>assets/</code> just moves it. The confirmation names the files that will be edited before anything moves.
</p>
<Callout variant="warning" title="A Java file moved to another package">
  The file moves and its <code>package</code> line stays as it was. Bennu flags the mismatch on that line, and
  <kbd>Alt</kbd> + <kbd>Enter</kbd> sets it to the new package — but whatever imports the class still names the old one, so a
  package move is not yet a one-gesture refactor.
</Callout>

<h2>Deleting, and getting it back</h2>
<p>
  <strong>Delete</strong> — <kbd>Del</kbd> or <kbd>⌫</kbd> on the focused row, or from the context menu — asks first and says
  what it is about to remove.
</p>
<p>
  It does not use the system trash: the files go into Bennu's own <strong>local history</strong>. That is why
  <kbd>Ctrl</kbd>/<kbd>Cmd</kbd> + <kbd>Z</kbd> with the tree focused puts them back — identically on every platform — why the
  toast that follows carries the same Undo, and why days later they are still in <strong>Local History › Deleted</strong>.
</p>
<Callout variant="tip" title="Two undo stacks, on purpose">
  <kbd>Ctrl</kbd>/<kbd>Cmd</kbd> + <kbd>Z</kbd> in the editor means "un-type that"; in the tree it means "un-delete that".
  Whichever has the focus answers.
</Callout>

<h2>The header and the menus</h2>
<p>
  The <strong>Project</strong> header carries a <strong>New</strong> menu, locating the open file in the tree, collapsing or
  expanding the whole tree, and an options menu.
</p>
<p>
  Right-clicking a file or folder opens: New · Open · Rename · Delete · Local History · Copy path · Reveal in Project · Reveal
  in File Explorer. <kbd>Shift</kbd> + <kbd>F10</kbd> — or the Menu key — opens the same menu on the focused row, so every entry
  is reachable without the mouse.
</p>
<dl class="meta-grid">
  <dt>Reveal in File Explorer</dt>
  <dd>
    Shows the row on disk: a folder as its listing, a file selected inside its folder. It follows <em>Settings → File Explorer →
    Open in the built-in explorer</em> — Arbor's own explorer window when that is on, the system file manager when it is not.
  </dd>
  <dt>Focus in Project</dt>
  <dd>
    Sends the tree somewhere from <em>outside</em> it — on a crate in the Cargo panel or a module in Dependencies — opening it on
    that folder, expanded, selected and with the keyboard focus on it, so the arrows carry on from there. Clicking any row, or
    opening any file, hands the selection back to whatever the editor shows.
  </dd>
</dl>

<h2>It follows the disk</h2>
<p>
  A <code>git checkout</code>, a <code>cargo new</code>, an <code>npm install</code>, another editor saving a file — all of them
  show up on their own, with nothing to press and nothing to reopen. Changes arrive in bursts, so a checkout touching four
  hundred files is one reload, not four hundred.
</p>
<Callout variant="info" title="Generated directories are not watched at all">
  <code>target</code>, <code>node_modules</code>, <code>.git</code>, <code>.svelte-kit</code>, <code>coverage</code> and their
  kind are not filtered afterwards but <em>unwatched</em>. A build writing into <code>target</code> would otherwise be a burst
  that never goes quiet — the tree refusing to settle at exactly the moment the machine is busiest.
</Callout>
