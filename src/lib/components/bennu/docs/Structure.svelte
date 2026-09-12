<script lang="ts">
  /**
   * Structure and Trees: the members the open file declares, and the two readings of its parse — the grammar's own
   * tree, and the model in Java's (or JSP's) vocabulary.
   */
  import Callout from '$lib/components/shared/ui/Callout.svelte';
</script>

<span class="eyebrow">Editor</span>
<h1>Structure &amp; trees</h1>

<p class="doc-lead">
  Two ways of looking at the file you have open rather than at the project: the members it declares, and the tree the parser
  actually built out of it.
</p>

<h2>Structure</h2>
<p>
  The <strong>Structure</strong> tool, on the left rail, lists the open file's symbols — types, methods and fields — grouped by kind,
  filterable, and sorted by position or by name. Click a symbol to jump to its declaration.
</p>
<ul>
  <li>A method with <code>@Override</code> carries an <strong>override marker</strong>, an up arrow, so the members that specialise a
    supertype stand out.</li>
  <li><strong>Collapse all</strong> and <strong>Expand all</strong> in the header fold or unfold the whole tree, as in the Project panel.</li>
</ul>

<h2>Trees</h2>
<p>
  <kbd>Alt</kbd> + <kbd>9</kbd> opens <strong>Trees</strong> on the right: two readings of the file in front of you, on two tabs.
</p>
<div class="feature-grid two-col">
  <div class="feature-card">
    <div class="fc-eyebrow">Why did it read it that way?</div>
    <div class="fc-title">Syntax</div>
    <div class="fc-desc">What the parser actually built, anonymous nodes and all.</div>
  </div>
  <div class="feature-card">
    <div class="fc-eyebrow">What does it mean?</div>
    <div class="fc-title">Model</div>
    <div class="fc-desc">The same parse in the language's own vocabulary, with resolved types.</div>
  </div>
</div>

<h3>Syntax</h3>
<p>
  The anonymous nodes — commas, keywords — are shown by default: they are noisy, and very often the answer. The ⧩ button hides them for the
  reading where they are not.
</p>
<ul class="prop-list">
  <li><strong>Field</strong>the part a node fills in its parent — the difference between "an identifier" and "the name of the method"</li>
  <li><em>invented</em>a node the parser had to make up to keep going</li>
  <li><em>truncated</em>a subtree cut short, rather than pretending the file ends there</li>
</ul>

<h3>Model</h3>
<p>
  The <strong>AST</strong>: the same parse in Java's vocabulary, all the way down — types, members and <strong>bodies</strong>, every
  statement and expression as <code>if</code>, <code>for each</code>, <code>call</code>, <code>local variable</code>, <code>binary</code>
  rather than as the grammar's node names. Four things separate it from Syntax, and all four are the point:
</p>
<div class="feature-grid two-col">
  <div class="feature-card">
    <div class="fc-title">Punctuation is gone</div>
    <div class="fc-desc">Commas, brackets and semicolons are not concepts.</div>
  </div>
  <div class="feature-card">
    <div class="fc-title">Wrappers are unwrapped</div>
    <div class="fc-desc">A call statement is a call, not an <code>expression_statement</code> holding one; <code>(a + b)</code> is an addition.</div>
  </div>
  <div class="feature-card">
    <div class="fc-title">Every child says its part</div>
    <div class="fc-desc"><code>condition</code>, <code>then</code>, <code>receiver</code>, <code>argument</code>, <code>returns</code>. A signature is rows — each parameter its own line, span and click — not a rendered string.</div>
  </div>
  <div class="feature-card">
    <div class="fc-title">Resolved types are shown</div>
    <div class="fc-desc"><code>conn : java.sql.Connection</code>. A bare name that is a <em>class</em> reads <code>Files → java.nio.file.Files</code>, an arrow for a colon: static versus instance, visible.</div>
  </div>
</div>
<p>
  Types, their annotations and modifiers each have a column, and a member nobody wrote is marked <em>generated</em> — a record's accessors
  and canonical constructor. They are part of what Bennu understands, so leaving them out would make the tree disagree with completion;
  selecting one takes you to the declaration that owes it.
</p>
<Callout variant="info" title="Nothing is dropped silently">
  A construct the model has no entry for keeps its grammar name and its children — the tree is never wrong, only occasionally less pretty.
  Types need the classpath, so on a project still indexing the tree is complete and untyped, and fills in as the index lands.
</Callout>

<h3>Model — a JSP</h3>
<p>
  A page has its own vocabulary, and the tab reads it in that: the <strong>libraries</strong> the page declares and what each
  <code>uri</code> resolved to, the <strong>tags</strong> with the library each came from, their <strong>attributes</strong> with the type
  the TLD declares, the expressions, the scriptlets and the includes. Three things it shows that the parse cannot:
</p>
<dl class="meta-grid">
  <dt>Nesting</dt>
  <dd>
    The JSP grammar is deliberately flat — an opening and a closing tag are siblings, which keeps a page with unbalanced markup colouring
    correctly. The model pairs them up, tolerantly: a close with no open, or a tag never closed, costs the rows below it nothing.
  </dd>
  <dt>Which library a tag is from</dt>
  <dd>
    <code>&lt;s:iterator&gt;</code> is a name until the page's own <code>&lt;%@ taglib %&gt;</code> says what <code>s</code> is. A prefix
    nobody declared says so in that column — the most common reason a taglib "stops working".
  </dd>
  <dt>What is an expression</dt>
  <dd>
    <code>value="%&#123;codice&#125;"</code> and <code>value="Codice"</code> are one shape to a grammar and opposite things to a reader, so
    the flavour — <em>OGNL</em>, <em>EL</em> — is a column rather than something to squint at the quotes for.
  </dd>
</dl>
<p>
  Page text is left out on purpose: a page is mostly prose and markup, and listing every run of it would bury the rows that carry meaning.
  The Syntax tab has them all.
</p>

<h3>Both tabs</h3>
<ul>
  <li>They follow the <strong>buffer</strong>, not the file on disk — the moment you want a tree is the moment you typed something that read
    differently than you expected.</li>
  <li>Selection travels both ways: clicking a node selects its bytes in the editor, and moving the caret opens the tree down to what holds
    it, scrolled into view.</li>
  <li>The filter matches kind, field and text at once, so "the method called <code>place</code>" is one query.</li>
  <li>Both read <strong>Java</strong> and <strong>JSP</strong> — pages, fragments and tag files. The page tree comes from the grammar colouring
    the file, so panel and colours cannot disagree.</li>
</ul>
<Callout variant="info" title="A file Bennu does not parse">
  Each tab says so in its own words — "no grammar for XML yet", "no declaration model for XML yet" — rather than showing an empty panel, which
  would read as a fact about your file instead of about the tool.
</Callout>
