<script lang="ts">
  /** Garrulus docs — writing and editing notes. */
</script>

<h1>Editing</h1>
<p class="doc-lead">
  The editor shows the note as it reads, not as it is spelled — except on the line the
  caret is on, which shows its markdown so you can edit it. Nothing is hidden from you;
  what you are looking at is the file.
</p>

<h2>Live preview</h2>
<p>
  Headings are sized, bold is bold, links read as their titles, tables are tables and
  code is highlighted. Put the caret on a line and that line reveals its syntax —
  the <code>##</code>, the <code>**</code>, the brackets — so there is never a mode to
  switch into in order to fix something.
</p>
<p>
  Saving is <kbd>Ctrl</kbd>+<kbd>S</kbd>, and leaving a note saves it too. What you
  typed is what lands on disk: the file is never reformatted on the way out, so a note
  Arbor has opened is byte-identical to the one another editor left behind.
</p>

<h2>Linking</h2>
<p>
  Links are written by hand, in the Obsidian spelling — the same file opens in either
  application and neither rewrites the other's links.
</p>
<ul>
  <li><code>[[Note]]</code> — a link, shown as the note's title.</li>
  <li><code>[[Note|call it this]]</code> — the same link, worded for the sentence it is in.</li>
  <li><code>![[Note]]</code> — an embed: that note's text appears here. With
    <code>#</code>, only that section.</li>
</ul>
<p>
  <kbd>Ctrl</kbd>+<kbd>Click</kbd>, or <kbd>Enter</kbd> with the caret on it, follows a
  link. <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>Click</kbd> opens it beside this note
  instead. <kbd>Alt</kbd>+<kbd>←</kbd> and <kbd>Alt</kbd>+<kbd>→</kbd> walk back and
  forward through where you have been.
</p>
<p>
  A link to a note that does not exist is styled differently and is <b>not</b> a
  mistake — following it creates that note. Writing the link you wish existed is the
  fastest way to make it exist. Those pending links are listed in Problems, so nothing
  gets lost by being written before it was created.
</p>
<p>
  The right panel shows <b>what links to this note</b>, each with the sentence it was
  linked from, plus <b>unlinked mentions</b>: other notes that name this one in plain
  text. One click turns a mention into a link.
</p>

<h2>Tags and fields</h2>
<p>
  <code>#tag</code> anywhere in the body is a tag, and <code>#nested/tag</code> nests.
  Typing <code>#</code> completes against the vocabulary already in the vault, which is
  what keeps a vault from ending up with <code>#todo</code>, <code>#to-do</code> and
  <code>#todos</code>.
</p>
<p>
  The block at the top of a note is its fields, rendered as a form: a dropdown where the
  type declares fixed values, a date where it declares a date. The button beside it
  shows the raw YAML for the times you would rather type it. Both write the same file.
</p>

<h2>Callouts</h2>
<p>
  A quote whose first line names a kind renders as a box with its own icon and colour:
</p>
<pre><code>&gt; [!warning] Do not do this on a worker thread
&gt; The reason, at whatever length it takes.</code></pre>
<p>
  <code>note</code>, <code>tip</code>, <code>important</code>, <code>warning</code>,
  <code>caution</code>, <code>question</code>, <code>example</code> and <code>quote</code> are
  recognised, in any case — and so are the words that mean the same things elsewhere
  (<code>info</code>, <code>hint</code>, <code>success</code>, <code>attention</code>,
  <code>danger</code>), because the same file is read here and on GitHub. Anything written after
  the marker becomes the callout's title; with nothing there, the kind names itself.
</p>
<p>
  The same boxes render wherever Arbor shows markdown — a <code>.md</code> opened in the code
  editor, or the markdown window — because it is one editor.
</p>

<h2>Tasks</h2>
<p>
  <code>- [ ]</code> is a task and <code>- [x]</code> is a done one; the checkbox is
  clickable in the rendered view. Every task in the vault is collected in the Tasks
  panel of the bottom dock, grouped by the note it lives in and by when it is due, so a
  task written in the middle of a design note is not lost in the middle of a design
  note.
</p>

<h2>Code, and the rest of the suite</h2>
<p>
  Fenced code is highlighted by language — <code>dig</code>, <code>merula</code>,
  <code>wgsl</code> and <code>ron</code> included, which no markdown renderer outside this
  application knows. Because Garrulus runs in the same application
  as the rest of Arbor, a fence can also be acted on: a <code>sql</code> block offers to
  open in Picus, and a file-and-line reference in a repository Arbor knows opens there —
  pinned to the revision it was written against, so a link in a note from six months ago
  still points at the line it meant.
</p>

<h2>Attachments</h2>
<p>
  An image pasted from the clipboard is written into the vault's attachments folder,
  named, and linked where the caret was. Pasting a URL over a selection turns the
  selection into a link, and pasting formatted text from a browser arrives as markdown
  rather than as a wall of HTML.
</p>

<h2>Changing a note after the fact</h2>
<ul>
  <li><kbd>F2</kbd> — <b>rename the note and every link to it</b>. What will change is
    shown before anything changes. Renaming by hand in a file manager is how a vault
    accumulates broken links; this is why the key exists.</li>
  <li><kbd>Alt</kbd>+<kbd>Enter</kbd> — <b>intentions</b> on the selection or the line:
    extract it into a new note leaving a link behind, give an untyped note a type, turn
    a paragraph into a callout, turn a line into a task, link a mention.</li>
  <li>Deleting a note moves it to the vault's own trash, where it can be put back
    without going through history.</li>
</ul>

<h2>Saving</h2>
<p>
  The editor saves on its own. <kbd>Ctrl</kbd>+<kbd>S</kbd> makes the moment explicit —
  useful just before syncing, since what has not been written cannot be sent.
</p>
