<script lang="ts">
  /** Garrulus docs — Getting Started. */
</script>

<h1>Garrulus</h1>
<p class="doc-lead">
  Garrulus is Arbor's notes product: a vault of markdown files you write in, link
  together and <b>keep in step across two machines</b>. The notes are ordinary
  <code>.md</code> files on disk, and they stay that way.
</p>

<h2>What a vault is</h2>
<p>
  A vault is a folder. Everything in it that ends in <code>.md</code> is a note; the
  folders you make inside it are yours to organise however you like. There is no
  database, no import step and no container format — which means the vault can be
  opened by any other markdown editor, backed up by copying it, and read on a machine
  that has never heard of Arbor.
</p>
<p>
  That is a deliberate constraint rather than a shortcut. A vault you cannot leave is a
  vault you have to trust completely before you start; a folder of markdown files asks
  nothing of you. It also makes moving an existing vault in a no-op: point Garrulus at
  it and it opens.
</p>
<p>
  <b>Open a vault</b> with <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>O</kbd>, from the
  command palette (<kbd>Ctrl</kbd>+<kbd>K</kbd>) or from the button on the empty window.
  The same dialog creates one, which writes a marker folder, the default note types and
  nothing else. Garrulus never opens a vault on its own — not even the one you had open
  last, which it offers instead: opening reads and indexes every note in the folder.
</p>

<h2>The folder Garrulus keeps</h2>
<p>
  Everything Garrulus stores <i>about</i> your vault lives in one directory inside it,
  <code>.arbor/garrulus/</code>. One dot-folder, so the whole of it can be inspected,
  backed up or deleted in one gesture:
</p>
<ul>
  <li><code>types/</code> — the note types and their templates. They are inside the
    vault on purpose: they travel with it, so the second machine already has them.</li>
  <li><code>vault.toml</code> — vault-scoped settings: the attachments folder, the
    daily-note folder, how links are written.</li>
  <li><code>devices.json</code> — when each machine last synced, which is what lets the
    sync button say <i>home, 3 minutes ago</i> instead of showing a bare timestamp.</li>
  <li><code>trash/</code> — notes you deleted, restorable without digging through
    history.</li>
</ul>
<p>
  The search index is <b>not</b> in there. It lives in Arbor's cache directory, is never
  synced, and can be deleted at any time — <i>Rebuild the index</i> in the palette
  reconstructs it from the notes, which are the only record.
</p>

<h2>A note</h2>
<p>
  A note is markdown with an optional block of YAML at the very top — the
  <b>frontmatter</b>, where its fields live. Garrulus renders that block as a form
  rather than as raw YAML, so a field with a fixed set of values is a dropdown instead
  of a string you can mistype, but on disk it stays ordinary YAML that any other editor
  reads.
</p>
<p>Beyond plain markdown, a note can carry:</p>
<ul>
  <li><b>Links</b> — <code>[[Another note]]</code>, or
    <code>[[Another note|what to call it here]]</code>. A link to a note that does not
    exist yet is not an error: following it creates the note. That is how most notes
    get made.</li>
  <li><b>Tags</b> — <code>#arbor</code>, <code>#geode/audio</code>, in the body or in
    the frontmatter.</li>
  <li><b>Tasks</b> — <code>- [ ]</code> and <code>- [x]</code>, collected across the
    whole vault in the Tasks panel.</li>
  <li><b>Callouts</b> — <code>&gt; [!warning]</code> and its siblings, rendered as
    boxes you can fold.</li>
  <li><b>Embeds</b> — <code>![[Another note]]</code> pulls that note's text in where it
    is written, and <code>![[Another note#A heading]]</code> pulls in one section.</li>
</ul>

<h2>Note types</h2>
<p>
  A note type says what a kind of note is made of: which fields it carries, where new
  ones land, what they are called and what they start with. A bug has an application, a
  severity and a status; a decision record has context, decision and consequences. Types
  are what turn a vault from a pile of prose into something you can filter, sort and
  count without giving up any of the prose. See <i>Note types</i>.
</p>

<h2>Keeping two machines in step</h2>
<p>
  The reason the product exists. A vault syncs to a destination — a private git
  repository, or a folder that something else already mirrors — and the state of that
  relationship is shown in exactly one place: the button in the title bar.
</p>
<p>
  The rule worth knowing before anything else is that <b>nothing writes without a
  click</b>. The background only ever looks; every byte that moves, in either direction,
  moved because you pressed something. See <i>Syncing</i>.
</p>

<h2>The zones</h2>
<ul>
  <li><b>Title bar</b> — the vault you are in, and the sync button.</li>
  <li><b>Activity bar</b> — Notes, Search, Tags and fields, Note types
    (<kbd>Ctrl</kbd>+<kbd>1</kbd>…<kbd>Ctrl</kbd>+<kbd>4</kbd>), then the views that
    take the whole centre: the table and the link graph.</li>
  <li><b>Sidebar</b> — the active section. In Notes: what you pinned, what you touched
    recently, then the tree.</li>
  <li><b>Centre</b> — note tabs and the note.</li>
  <li><b>Right panel</b> — what describes the <i>note</i> rather than the vault:
    what links to it, its outline, its fields.</li>
  <li><b>Bottom dock</b> (<kbd>Ctrl</kbd>+<kbd>J</kbd>) — Tasks, Problems, Conflicts and
    the note's History.</li>
  <li><b>Status bar</b> — the vault, how many notes it holds, where sync stands, and the
    path of the note in front.</li>
</ul>

<h2>Finding things</h2>
<p>
  Two boxes, and they answer different questions.
  <kbd>Ctrl</kbd>+<kbd>K</kbd> is the <b>command palette</b>: what can I do.
  <kbd>Ctrl</kbd>+<kbd>O</kbd> is the <b>quick switcher</b>: where is the note called
  something like this. They are separate because merging them produces a ranking in
  which a verb and a title compete, and neither question gets a good answer.
</p>
<p>
  <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>F</kbd> is the third: search across the whole
  vault, full text and typed filters in one query, so
  <code>type:bug status:open profile</code> is a valid thing to type. Because the fields
  are declared by the note type, the filter half can be checked and completed as you
  write it.
</p>
<p>
  This documentation can leave the window: the button beside the title exports it as a
  Markdown README or as a self-contained HTML page.
</p>
