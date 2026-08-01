<script lang="ts">
  /** Garrulus docs — note types, templates and the views they make possible. */
</script>

<h1>Note types</h1>
<p class="doc-lead">
  A note type says what a kind of note is made of: the fields it carries, where new ones
  land, what they are called and what they start with. It is what lets a vault hold
  records — bugs, decisions, design notes — without any of them stopping being prose.
</p>

<h2>Why a bug is a note</h2>
<p>
  A bug looks like a database row: fields, a status, something to filter and sort. It is
  a note here anyway, for reasons that are worth stating because they decide almost
  everything else about the product.
</p>
<ul>
  <li><b>A database cannot be merged.</b> The whole point of the vault is two machines,
    and a binary file is one opaque blob: any concurrent change is a whole-file
    conflict whose only answers are "keep mine" and "keep theirs". Losing an afternoon
    of records that way is precisely the failure this is built to prevent. Markdown
    conflicts are per note, and usually per paragraph.</li>
  <li><b>A row cannot be linked.</b> Bugs and design notes are the most connected things
    in a vault — they point at each other, at decisions, at commits, at the day they
    were written. Moving them out of the notes would carve the most-linked content out
    of the linking system.</li>
  <li><b>The body is prose anyway.</b> Steps to reproduce, an analysis, a stack trace, a
    screenshot. Half the record would end up in a text column no editor understands.</li>
</ul>
<p>
  What a type adds back is the thing plain text lacks: nothing in a text file stops you
  writing <code>severity: bloker</code>. A field written through the type's form is
  checked against what the type declares, and anything already wrong shows up in
  Problems.
</p>

<h2>What a type declares</h2>
<ul>
  <li><b>A name, an icon and a colour.</b> The colour follows the note everywhere — its
    tab, its row in the tree, its result in search, its node in the graph — so kind is
    recognisable before anything is read.</li>
  <li><b>Where new notes land</b>, and <b>what they are called</b>: a filename pattern,
    so notes of a kind are named consistently without anyone having to remember the
    convention.</li>
  <li><b>How an existing note is recognised</b> as being of this type — by a field in
    its frontmatter, or by the folder it sits in.</li>
  <li><b>Its fields.</b> Each has a key, a label in your own words, and a kind: text,
    number, yes/no, date, a list of values, tags, a link to another note, or a link to
    something in a repository. A field can be required, can have a default, and can
    declare the values it accepts.</li>
  <li><b>A template</b> — the headings and prompts a new note of this type starts with,
    with the caret placed where writing actually begins.</li>
  <li><b>A layout</b> — which panels open with a note of this kind. A bug opens with its
    links and its tasks; a design note opens wide, with the graph.</li>
</ul>
<p>
  Types live <b>inside the vault</b>, under <code>.arbor/garrulus/types/</code>. That is
  deliberate: they sync with it, so the templates you refined on one machine are already
  on the other one.
</p>

<h2>Making notes with them</h2>
<p>
  <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>N</kbd> creates a note of a type: pick the type,
  give it a title, and the note arrives in the right folder with the right name, its
  fields ready to fill and its headings already written. Each type is also its own
  command palette entry, so <i>New Bug</i> is one thing to type rather than two steps.
</p>
<p>
  A note that already exists can be given a type at any time. Doing so adds what the
  template has and the note lacks, and opens the fields — it never touches what is
  already written.
</p>

<h2>What typed fields buy</h2>
<ul>
  <li><b>A form instead of YAML.</b> A status is a dropdown of the statuses that exist,
    which is the difference between a field you can filter on and a field you can
    misspell.</li>
  <li><b>Search that understands them.</b> <code>type:bug status:open</code> is part of
    an ordinary search query, and because the fields are declared, the query can be
    completed and checked as you write it rather than failing silently.</li>
  <li><b>A table.</b> Any set of notes — a folder, a tag, a type, a search — can be
    shown as a grid whose columns are the type's fields, sortable, filterable, and
    editable in place. Editing a cell edits the note.</li>
  <li><b>A board.</b> A type whose fields include one marked as the grouping field gets
    a board grouped by it: triage as columns, dragging a card writes the field.</li>
  <li><b>Saved views.</b> A query worth returning to becomes an entry in the sidebar,
    rendered as a list, a table or a board.</li>
</ul>

<h2>The types a new vault starts with</h2>
<p>
  A new vault is created with a set of types you are meant to edit rather than obey:
  <b>bug</b>, <b>improvement</b>, <b>game design</b>, <b>daily note</b>, <b>meeting</b>,
  <b>decision</b> (context / decision / consequences) and <b>snippet</b>. Change their
  fields, rename them, delete the ones you will never use — they are the vault's, not
  the application's.
</p>
<p>
  Notes with no type are entirely legitimate and stay that way. Most notes are just
  notes; the typed ones are the ones you will later want to count.
</p>
