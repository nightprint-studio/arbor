<!-- Bennu docs — the editing surface itself: what a buffer does while you type in it. -->
<h1>The editor</h1>
<p class="doc-lead">
  What a file looks like and behaves like once it is open — highlighting, folding, guides, the
  overview ruler, and what happens when you save it or when something else changes it underneath
  you. Moving <em>around</em> the code is <em>Navigation</em>; what the editor offers you as you
  type is <em>Completion</em>.
</p>

<h2>Languages</h2>
<p>
  Three tiers, in descending order of what they know about the file:
</p>
<ul>
  <li><strong>Java</strong>, <strong>JSP</strong> and <strong><code>.dig</code></strong> parse with a
    real grammar: semantic highlighting and code folding. Java and JSP add navigation and
    index-backed completion; <code>.dig</code> completes from its own vocabulary (below).</li>
  <li><strong>HTML</strong> and <strong>JSON</strong> highlight and fold.</li>
  <li><strong>Markdown</strong> is a case of its own — see below.</li>
  <li><strong>Rust</strong>, <strong>C</strong>, <strong>C++</strong> and <strong>Python</strong>
    are coloured from a mode of their own, so a <code>.py</code> build script or a
    <code>.cpp</code> beside a JNI library reads as code with no server installed at all — and
    gains completion, hover and semantic colour the moment clangd or pyright is.</li>
  <li><strong>TOML</strong>, <strong>RON</strong>, XML, YAML,
    <code>.properties</code>, CSS/SCSS/LESS, JavaScript/TypeScript, shell and SQL highlight.
    Colour only: navigation and completion want a language server, and until one is wired those
    actions are hidden rather than offered and silent.</li>
  <li><strong>DTD</strong> (<code>.dtd</code>, and the <code>.ent</code> / <code>.mod</code>
    fragments a large one is split into) has a mode of its own: a DTD is not XML —
    <code>&lt;!ELEMENT</code> is a malformed tag to an XML highlighter — and it is what the
    <code>struts.xml</code>s and <code>.tld</code>s of a legacy project are written against. The
    declarations, the name each one <em>declares</em>, and the parameter entities
    (<code>%common;</code>) a real DTD is mostly made of each read apart.</li>
</ul>
<p>
  <strong>SQL</strong> is highlighted per <strong>dialect</strong>, because the engines disagree
  about string quoting: Oracle's <code>q'[…]'</code> and PostgreSQL's <code>$$ … $$</code> are each
  a broken string under the other's rules, and getting it wrong paints the rest of a file as one
  literal. Nothing inside a <code>.sql</code> file says which engine it targets, so it is a setting —
  <strong>Settings → Editor → SQL → Dialect</strong>. The default, <em>Portable</em>, uses the rules
  valid on both.
</p>
<h2>Tabs</h2>
<ul>
  <li>Open a file from the <strong>Project</strong> tree or a <strong>Find in project</strong> hit — it joins the tab strip.</li>
  <li>Switch tabs with a click; the <strong>×</strong> closes one and a neighbour takes focus.</li>
  <li>A tab keeps its cursor, its scroll position and its <strong>undo history</strong> while it is
    open, so coming back to a file lands you where you left it and <kbd>Ctrl/Cmd</kbd> + <kbd>Z</kbd>
    still takes back what you typed there. Closing the tab lets them go.</li>
</ul>
<h2>Markdown</h2>
<p>
  A <code>.md</code> opens <strong>rendered</strong>, in the same live-preview editor Garrulus's
  notes use: headings are sized, links read as their titles, tables are tables, images and video
  appear, and fenced code is highlighted by language — including the ones this app is built
  around and no markdown renderer has ever heard of: <code>dig</code>, <code>merula</code>,
  <code>wgsl</code>, <code>ron</code>. It is still the file and still an editor —
  put the caret on a line and that line shows its markup, so a typo is fixed where you found it.
  The button in the toolbar (or <em>Markdown: edit the source</em> in the Command Palette)
  switches to the code editor for when the markup itself is the work; the choice is remembered.
</p>
<p>
  <strong>Alerts</strong> render as callouts — a coloured band with an icon and a title:
</p>
<pre><code>&gt; [!WARNING]
&gt; Deploying this on a Friday is how the weekend ends.</code></pre>
<p>
  GitHub's five (<code>NOTE</code>, <code>TIP</code>, <code>IMPORTANT</code>,
  <code>WARNING</code>, <code>CAUTION</code>) and the Obsidian words that mean the same things
  (<code>info</code>, <code>hint</code>, <code>success</code>, <code>attention</code>,
  <code>danger</code>, <code>question</code>, <code>example</code>, <code>quote</code>) are
  recognised, in any case. Anything written after the marker becomes the callout's title.
</p>
<p>
  <strong>A table stays a table.</strong> Click a cell and you edit that cell — only the cell you
  are in shows its own markdown, everything around it stays rendered. The strip under the table
  adds and removes rows and columns and sets a column's alignment, <kbd>Tab</kbd> and
  <kbd>Enter</kbd> walk the grid, and <kbd>Esc</kbd> leaves it. The toolbar's <strong>⊞</strong>
  inserts a new one: point at the grid to say how big. Editing a table rewrites its markdown
  normalised — one space each side of every cell — so a hand-aligned table loses its padding the
  first time it is edited here.
</p>
<p>
  <strong>Links work.</strong> An <code>http</code> one opens in the browser, a path opens the file
  in a tab — resolved against the document's own folder, so <code>./notes/api.md</code> means the
  one beside it — and a <code>#anchor</code> jumps to that heading, in this file or in the one it
  opens. Bare URLs count: both <code>&lt;https://…&gt;</code> and one written on its own.
</p>
<p>
  Following one is <kbd>Ctrl</kbd> / <kbd>Cmd</kbd> + click, the same gesture that follows a symbol
  in code. A plain click puts the caret where you clicked, because in a document you are editing
  that is what a click has to mean — and the hand cursor appears only while the key is held, so it
  never promises a jump that a click alone won't make.
</p>
<p>
  <strong>Typing <code>](</code> offers the targets.</strong> Every file in the project, written
  relative to this document, and every heading in it — picked from the same list the jump reads,
  so an id that is offered is an id that lands. Typing <code>#</code> first narrows it to the
  headings alone, where each one shows its title beside its id.
</p>
<p>
  <strong>A heading needs no id: its text is one.</strong> Lower-cased, punctuation dropped, spaces
  turned into dashes — GitHub's rule, character for character, so an anchor copied from a table of
  contents generated there lands. <code>## Perché il CST</code> is reached by
  <code>[…](#perché-il-cst)</code>; two headings with the same text give
  <code>#titolo</code> and <code>#titolo-1</code>; an underlined heading has an id like any other.
  A <code>path.md#anchor</code> works the same way in the file it opens.
</p>
<p>
  Where a hand-written anchor and a generated one differ, both work. Accents may be left out
  (<code>#perche-il-cst</code>), and so may the doubled dashes GitHub produces where punctuation
  stood between two spaces — <code>## Pipelines — CI / CD</code> answers to
  <code>#pipelines--ci--cd</code> and to <code>#pipelines-ci-cd</code>. A <code>#</code> comment
  inside a fenced code block is never mistaken for a heading.
</p>
<p>
  A <code>mermaid</code> fence is <strong>drawn</strong> rather than printed — flowcharts,
  sequence diagrams, state machines — in the theme's own colours, and the caret inside it brings
  the source back like every other block here, <strong>highlighted</strong>: the arrows, the node
  labels and the diagram type each read apart, which is what you are scanning for while you write
  one. A <code>.mmd</code> file opens with the same colouring. A diagram that does not parse says
  so where the picture would have been, in mermaid's words, naming the line.
</p>

<h2>Code folding</h2>
<p>
  Braced blocks (classes, methods, blocks) and block comments fold from the gutter chevrons; the head
  line stays visible. In an indentation-delimited language like <code>.dig</code> the body folds from
  the end of its header line instead, with the same effect. Folding is computed live from the syntax
  tree — no indexing needed.
</p>
<p>
  <strong>Settings → Editor → Folding</strong> turns the gutter off entirely, and can collapse a
  file's <strong>block comments as it opens</strong> — the licence header and the documentation
  above every method, which on a legacy file can be most of what is on screen. That is an opening
  state, not a rule: nothing re-folds what you unfold.
</p>
<h2>What the surface looks like</h2>
<p>
  <strong>Settings → Editor → Appearance</strong> owns the rest of it: the font size, the
  line-number gutter, the tint on the caret's line, word wrap, and whether spaces and tabs are
  drawn as glyphs. Each applies to the file already in front of you, not to the next one you open.
</p>
<h2>Rainbow brackets</h2>
<p>
  Every <code>()</code>, <code>[]</code> and <code>&#123;&#125;</code> is tinted by its nesting depth — a
  matching open/close pair shares a colour — so a block tells you at a glance which bracket it closes.
  Brackets inside strings and comments are left alone.
</p>
<h2>Indentation guides</h2>
<p>
  A vertical line marks each indent level, tinted the same rainbow colour as the bracket that opens
  its block, and the guide of the block the caret sits in is fully highlighted — so you can see at a
  glance which block a line belongs to and where it closes. Toggle it in
  <strong>Settings → Editor → Indentation guides</strong>.
</p>
<h2>Sticky scroll</h2>
<p>
  As you scroll into a long body, the enclosing declarations — the class signature, then the method —
  pin to the top of the editor so you never lose the context. Click a pinned line to jump back to it.
  Toggle it in <strong>Settings → Editor → Sticky scroll</strong>.
</p>
<h2>Scrollbar overview</h2>
<p>
  The right-edge <strong>overview strip</strong> replaces the plain scrollbar: every error and warning
  is a coloured bar at its position in the file, so you see where the problems are at a glance. Hover
  the strip to preview the file at that spot, drag it to scroll, and click a mark to jump straight to
  that line — the caret lands there, ready for a quick-fix. A thumb shows where in the file you are.
  Toggle the strip in <strong>Settings → Editor → Scrollbar overview</strong>.
</p>
<p>
  The marks cover the <strong>compiler's</strong> errors and warnings too, not just the live analysis:
  after a build, whatever <code>javac</code>, Maven or <code>cargo</code> reported for the open file is
  a mark on the strip and a squiggle on the line, alongside everything else. A rebuild replaces them,
  so a fixed error clears its mark.
</p>
<h2>File health</h2>
<p>
  A badge in the editor's top-right corner shows the current file's error and warning counts (a green
  check when it's clean), mirroring the marks on the scrollbar overview.
</p>
<h2>Indentation</h2>
<p>
  The footer shows the active indentation as <em>Spaces: N</em> or <em>Tab Size: N</em>. Click it (or
  focus it and press <kbd>↑</kbd>/<kbd>↓</kbd>) to switch between <strong>spaces and tabs</strong> and
  pick the <strong>tab width</strong> (2 / 4 / 8). The change applies to the open editor immediately.
</p>
<h2>Reformat</h2>
<p>
  <kbd>Alt</kbd> + <kbd>Shift</kbd> + <kbd>F</kbd> reformats the open file. A language with a
  <strong>language server</strong> is formatted by it (Rust by <code>rustfmt</code>); <strong>Java</strong>
  is formatted by Bennu, which re-indents every line to its nesting, strips trailing whitespace and
  collapses runs of blank lines — using the indentation the footer shows.
</p>
<p>
  It deliberately stops there: it never rewraps a long line, reorders anything, adds or removes
  braces, or changes the spacing inside an expression. Those are the rules that can be wrong —
  <code>a &lt; b</code> and <code>Map&lt;K, V&gt;</code> differ by context, <code>-1</code> and
  <code>a - 1</code> by parse — and a formatter that occasionally rewrites an expression is one
  nobody dares run on inherited code. Comments and text blocks are left exactly as written, inside
  and out, and a file that doesn't parse still formats.
</p>
<h2>Optimize imports</h2>
<p>
  <kbd>Ctrl</kbd> + <kbd>Shift</kbd> + <kbd>O</kbd> on a Java file drops the imports the file does
  not use and puts what is left in order: everything else first, then <code>javax</code> and
  <code>java</code>, then the static imports, alphabetical inside each group and a blank line
  between them. Duplicates collapse. It lands as one undo step.
</p>
<p>
  What counts as unused is the same judgement the <code>unused-import</code> warning makes, so the
  command and the squiggle can never disagree — which also means it inherits that judgement's
  caution: an import named only in a Javadoc counts as used, and a <code>static</code> or wildcard
  import is never removed, only moved. It will not fold several imports of one package into a
  wildcard (that can change what a simple name resolves to), and it will not add a missing import —
  that one is <kbd>Alt</kbd> + <kbd>Enter</kbd>'s.
</p>
<p>
  A file with a comment written among its imports is left alone. A comment sits above the import it
  was written for, and reordering would strand it above a different one.
</p>
<h2>Parameter and inlay hints</h2>
<p>
  Inside a call's argument list, a strip above the line shows the <strong>signature</strong> of the
  method with the argument you're on picked out — and it follows the commas as you type. Both engines
  answer it: Bennu's own for Java, the language server for everything else.
</p>
<p>
  <strong>Inlay hints</strong> (Settings → Editor) draw what the code doesn't say:
  the parameter name in front of each argument that doesn't already carry it —
  <code>transfer(source: from, target: to, amount: 500)</code> — and the type a <code>var</code> was
  inferred as. They are not part of the file: the caret can't land in one, they aren't copied with a
  selection, and no offset shifts. An argument that already says the name, a lambda, or a long
  expression is left alone.
</p>
<h2>Emmet</h2>
<p>
  In JSP and HTML files, type an <strong>Emmet abbreviation</strong> and press <kbd>Tab</kbd> to
  expand it into markup — <code>ul>li.item*3</code> becomes a list, <code>div#app</code> a div with
  an id, <code>a[href]</code> a link. When the caret isn't on a valid abbreviation, <kbd>Tab</kbd>
  just indents as usual.
</p>
<h2>Pasting into a string</h2>
<p>
  Paste inside a Java <code>"…"</code> and the text is <strong>escaped</strong> as it lands:
  quotes and backslashes are escaped, tabs become <code>\t</code>. Paste something that
  <strong>spans several lines</strong> and it becomes concatenated literals, one per line, aligned
  under the opening quote and joined with <code>+</code> — the shape you would have typed by hand,
  because a <code>"…"</code> cannot span lines in Java:
</p>
<pre><code>{`String q = "SELECT *\\n" +
           "FROM \\"user\\"";`}</code></pre>
<p>
  A <strong>text block</strong> (<code>"""</code>) is treated differently, because it exists to avoid
  exactly that: newlines stay newlines, pasted lines are indented to match the block, and only the
  quotes that would close it early are escaped. Inside a <code>'…'</code> character literal the text
  is escaped and never split.
</p>
<p>
  Everywhere else — in code, in a comment, or just past the closing quote — a paste arrives exactly
  as it left.
</p>
<p>
  Past <strong>500 lines</strong> the text is still escaped but no longer split: the newlines stay
  inline in a single literal. A concatenation of thousands of pieces is one expression nested
  thousands of levels deep, which is more than the tools that read it can walk.
</p>
<p>
  Past <strong>64&nbsp;KB</strong> the paste is <strong>refused</strong>, with a note at the caret
  saying so. That is the most a compiled string constant can hold however it is written — splitting
  it changes nothing, because the compiler joins the pieces back into one constant — so there is no
  arrangement of that text that would build, and inserting it would only mean finding that out later.
</p>
<h2>Saving</h2>
<p>
  <strong>Autosave is on by default</strong>: a modified file is written to disk automatically — a
  short moment after you stop typing, when you switch to another tab, and when the window loses focus
  (IntelliJ-style). You can still save explicitly with <kbd>Ctrl</kbd> + <kbd>S</kbd>. Turn autosave
  off in Settings → Editor → <strong>Autosave</strong> to save only on <kbd>Ctrl</kbd> + <kbd>S</kbd>;
  the choice persists across sessions.
</p>
<h2>Save</h2>
<p>
  <kbd>Ctrl</kbd> + <kbd>S</kbd> writes the current file to disk in the project's encoding. Rename
  applies and saves its edits the same way.
</p>
<h2>Files changed outside Bennu</h2>
<p>
  Bennu watches the files you have open — another editor, a <code>git checkout</code>, a code
  generator, a build — and never writes over a change it didn't make.
</p>
<ul>
  <li>If your tab has <strong>no unsaved edits</strong>, the new content is picked up
    <strong>silently</strong>. There is nothing to lose and nothing to decide.</li>
  <li>If your tab <strong>does</strong> have unsaved edits, both versions matter, so Bennu stops
    and asks: <strong>Keep my edits</strong> (overwrite what's on disk) or <strong>Reload from
    disk</strong> (discard yours). <em>Not now</em> defers the choice.</li>
</ul>
<p>
  While a file is waiting on that decision its tab is badged <strong>disk</strong> and
  <strong>autosave is paused for it</strong> — so an unattended timer can't pick a side for you.
  Nothing else is affected: every other tab keeps autosaving normally.
</p>
<p>
  The check also guards the save itself, not just the warning: a write whose file moved underneath
  is <strong>refused</strong> rather than applied, and the toast says so. That holds for
  <kbd>Ctrl</kbd> + <kbd>S</kbd>, autosave, save-on-tab-switch and the multi-file writes a
  <strong>Rename</strong> performs.
</p>
<p>
  A file <strong>deleted</strong> under an edited buffer is not treated as a conflict to resolve:
  your buffer is the last copy, so saving simply recreates the file.
</p>
