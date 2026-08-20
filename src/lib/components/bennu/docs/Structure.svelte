<!-- Bennu docs — the two structural views of the open file: Structure and Trees. -->
<h1>Structure &amp; trees</h1>
<p class="doc-lead">
  Two ways of looking at the file you have open rather than at the project: the members it
  declares, and the tree the parser actually built out of it.
</p>

<h2>Structure</h2>
<p>
  The <strong>Structure</strong> tool (left rail) lists the active file's symbols — types, methods and
  fields — grouped by kind and filterable, sortable by position or name. Click a symbol to jump the
  editor to its declaration. Methods carrying an <code>@Override</code> annotation show an
  <strong>override marker</strong> (an up arrow), so the members that specialise a supertype stand
  out at a glance. The header carries <strong>Collapse all</strong> / <strong>Expand all</strong>
  chevrons to fold or unfold the whole tree at once, just like the Project panel.
</p>
<h2>Trees</h2>
<p>
  <kbd>Alt</kbd> + <kbd>9</kbd> opens <strong>Trees</strong> on the right — two readings of the
  file in front of you, on two tabs.
</p>

<h3>Syntax</h3>
<p>
  What the parser actually built. It answers one question — <em>why did it read it that way?</em>
  — which is why the anonymous nodes, the commas and the keywords, are shown by default: they are
  noisy, and they are very often the answer. The ⧩ button hides them for the reading where they
  are not. Each row shows the <strong>field</strong> a node fills in its parent when it has one,
  which is the difference between “an identifier” and “the name of the method”. A node the parser
  had to invent to keep going is marked <em>invented</em>, and a subtree that was cut short says
  <em>truncated</em> rather than pretending the file ends there.
</p>

<h3>Model</h3>
<p>
  The <strong>AST</strong> — the same parse read in Java’s vocabulary, all the way down. Types,
  members, and the <strong>bodies</strong>: every statement and every expression, as
  <code>if</code>, <code>for each</code>, <code>call</code>, <code>local variable</code>,
  <code>binary</code> rather than as the grammar’s node names.
</p>
<p>Four things separate it from the Syntax tab, and all four are the point:</p>
<ul>
  <li><strong>Punctuation is gone.</strong> Commas, brackets and semicolons are not concepts.</li>
  <li><strong>Wrappers are unwrapped.</strong> A call statement is a call, not an
    <code>expression_statement</code> containing one; <code>(a + b)</code> is an addition, not a
    parenthesis around one.</li>
  <li><strong>Every child says what part it plays</strong> — <code>condition</code>,
    <code>then</code>, <code>receiver</code>, <code>argument</code>, <code>returns</code>. A
    signature is <em>rows</em>, not a rendered string: each parameter has its own line, its own
    span, and can be filtered and clicked like anything else.</li>
  <li><strong>Resolved types are shown</strong>, which a parse tree cannot hold at all:
    <code>conn : java.sql.Connection</code>. Where a bare name turns out to be a
    <em>class</em> rather than a value the row says <code>Files → java.nio.file.Files</code> with
    an arrow instead of a colon — that is the static-versus-instance distinction, visible.</li>
</ul>
<p>
  Types and their annotations are shown, modifiers get their own column, and a member nobody
  wrote is marked <em>generated</em>: a record’s accessors and canonical constructor. Those are
  genuinely part of what Bennu understands, so leaving them out would make the tree disagree with
  completion; selecting one takes you to the declaration that owes it rather than pretending it
  has source of its own.
</p>
<p>
  Nothing is dropped silently. A construct the lowering has no entry for keeps its grammar name
  and its children rather than disappearing — the tree is never wrong, only occasionally less
  pretty.
</p>
<p>
  Type annotations need the classpath, so on a project that is still indexing the tree is
  complete and untyped rather than absent, and fills in as the index lands.
</p>

<h3>Model — a JSP</h3>
<p>
  A page has its own vocabulary and the tab reads it in that: the <strong>libraries</strong> the
  page declares and what each <code>uri</code> resolved to, the <strong>tags</strong> with the
  library each one came from, their <strong>attributes</strong> with the declared type the TLD
  gives them, the expressions, the scriptlets and the includes.
</p>
<p>Three things it shows that the parse cannot:</p>
<ul>
  <li>
    <strong>Nesting.</strong> The JSP grammar is deliberately flat — an opening tag and its
    closing tag are siblings, which is what keeps a page with unbalanced markup colouring
    correctly instead of collapsing into one error. The model pairs them up, tolerantly: a close
    with no open, or a tag the page never closes, costs the rows below it nothing.
  </li>
  <li>
    <strong>Which library a tag is from.</strong> <code>&lt;s:iterator&gt;</code> is a name until
    the page’s own <code>&lt;%@ taglib %&gt;</code> line says what <code>s</code> is. A prefix
    nobody declared says so in that column, which is the most common reason a taglib “stops
    working”.
  </li>
  <li>
    <strong>What is an expression.</strong> <code>value="%&#123;codice&#125;"</code> and
    <code>value="Codice"</code> are the same shape to a grammar and opposite things to a reader,
    so the flavour — <em>OGNL</em>, <em>EL</em> — is a column rather than something to squint at
    the quotes for.
  </li>
</ul>
<p>
  Page text is left out on purpose: a page is mostly prose and markup, and listing every run of
  it would bury the rows that carry meaning. The Syntax tab has them all.
</p>

<h3>Both tabs</h3>
<p>
  They follow the <strong>buffer</strong>, not the file on disk, because the moment you want a
  tree is the moment you have typed something that read differently than you expected. Selection
  travels both ways: clicking a node selects its bytes in the editor, and moving the caret opens
  the tree down to what holds it — and scrolls to it, since a node revealed below the fold is a
  reveal you cannot see. The filter box matches on kind, on the field column and on the text at
  once, so “the method called <code>place</code>” is one query rather than two.
</p>
<p>
  Both draw whatever Bennu can read — <strong>Java</strong> and <strong>JSP</strong> (pages,
  fragments and tag files). The page tree comes from the same grammar that is colouring the file
  in front of you, so the panel and the colours can never disagree about what it is. For a file
  Bennu edits but does not parse, each tab says so in its own words (“no grammar for XML yet”,
  “no declaration model for XML yet”) instead of showing you an empty panel, because the first is
  a fact about the tool and the second reads as one about your file.
</p>
