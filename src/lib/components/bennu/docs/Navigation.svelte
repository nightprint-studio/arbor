<!-- Bennu docs — moving around the code: find, go-to, usages, hover. -->
<h1>Navigation</h1>
<p class="doc-lead">
  Getting to the thing you are thinking of, from wherever you are. Everything here works from the
  keyboard, and everything here answers from the index rather than from the open tabs — so it
  finds what you have never opened.
</p>

<h2>Find</h2>
<p>
  <kbd>Ctrl</kbd> + <kbd>F</kbd> searches the current file. <kbd>Ctrl</kbd> + <kbd>Shift</kbd> +
  <kbd>F</kbd> opens <strong>Find in project</strong> — a backend-powered search across the whole
  project with <strong>Match case</strong>, <strong>Whole word</strong> and <strong>Regex</strong>
  toggles beside the field (and, on a workspace, a fourth reaching into every member project),
  grouping hits by file with the match highlighted. Results stream in as the scan finds them, so a
  large project fills the list instead of making you wait for it.
</p>
<p>
  The selected hit is shown <strong>in context</strong> beside the list — the lines around it, with
  the match highlighted — which is what tells four identical-looking lines apart without opening
  four files. ↑/↓ move the selection (and the preview follows), <kbd>Enter</kbd> opens the hit.
  If a word is <strong>selected</strong> in the editor, it pre-fills the search field (both here and
  in Find-in-file).
</p>
<p>
  The header row is everything that decides <strong>what is searched</strong>. The
  <strong>Source</strong> picker — <strong>Project</strong>, <strong>Project &amp;
  dependencies</strong>, <strong>Dependencies</strong> — says whose text is read. Then the two
  narrowings: the <strong>module</strong>, on a multi-module build, and a <strong>file
  mask</strong> (<code>*.java</code>, or several at once as <code>*.jsp, *.tag</code>). Those two
  filter what came back rather than what is scanned, so changing either re-lists instantly instead
  of re-running the search, and both are <strong>remembered per project</strong>. The count
  between them says how many of the matches survived them.
</p>
<p>
  Reading the <strong>dependency jars</strong> — their XML, schemas, tag libraries and property
  files — is how you find which artifact declares the interceptor or the bean you are looking at.
  Those hits are <strong>tinted</strong> and named by their <strong>artifact</strong>, arrive
  after the project's own, and opening one extracts it read-only. It is per-search rather than a
  setting: every candidate entry has to be decompressed to be read, so it is a cost you take for
  the question you are asking now.
</p>
<h2>Back, Forward and the places you have been</h2>
<p>
  <kbd>Ctrl</kbd> + <kbd>Alt</kbd> + <kbd>←</kbd> goes <strong>back</strong> to where you jumped
  from, <kbd>Ctrl</kbd> + <kbd>Alt</kbd> + <kbd>→</kbd> forward again. A stop is recorded when an
  <strong>action</strong> navigates — a go-to, a usage, a structure or find hit, a diagnostic, a
  switch to another tab — and never when the caret merely moves: arrow keys, a click, page-down and
  scrolling are reading, not navigation, so the history stays a list of places you chose to go to.
  Each jump remembers <strong>both ends</strong>, so the first Back lands exactly where you left,
  down to the column, and navigating after a Back starts a new branch the way a browser does.
</p>
<p>
  <kbd>Ctrl</kbd> + <kbd>Shift</kbd> + <kbd>Backspace</kbd> goes back to where you were
  <strong>typing</strong> — a separate history, because "where was I reading" and "where was I
  editing" are different questions. Press it again to walk further back through the session's edits.
</p>
<p>
  <kbd>Ctrl</kbd> + <kbd>Shift</kbd> + <kbd>E</kbd> opens <strong>Recent locations</strong>: the same
  history as a list, most recent first, each row showing the line you were on. Type to filter by
  file name or by the text of the line, ↑/↓ to move, <kbd>Enter</kbd> to go. <strong>Edited
  only</strong> keeps the places you changed — which is how you find the file you were working on
  before the interruption. It is what you reach for instead of pressing Back five times and reading
  four screens on the way.
</p>

<h2>Go to line</h2>
<p>
  <kbd>Ctrl</kbd> + <kbd>G</kbd> opens the go-to-line box — type <code>42</code> or
  <code>42:8</code> (line:column) and press <kbd>Enter</kbd>.
</p>
<h2>Go to declaration</h2>
<p>
  Put the caret on a Java <strong>symbol</strong> — a class, method, field or local — and press
  <kbd>Ctrl</kbd> + <kbd>B</kbd> (or <kbd>Ctrl</kbd> + click, or the right-click menu) to jump to its
  declaration. If you're <strong>already on the declaration itself</strong> — a method signature, or
  the declaration of a variable, class or record — jumping would be a no-op, so the same gesture shows
  its <strong>usages</strong> instead (like IntelliJ). On a JSP form or link <strong>action
  reference</strong> — an <code>action="…"</code> value or a path like
  <code>/do/Category/viewTree</code> — it jumps to where the action is declared: the Struts config
  fragment, or its view JSP; if it resolves only to an implementation class, the class name is shown.
  It answers from the project index / config graph, so it works once the index is warm and stays quiet
  when a symbol can't be resolved.
</p>
<p>
  In a <strong>Struts config XML</strong> the same gesture works on a <code>&lt;result&gt;</code>: a
  JSP path (<code>/WEB-INF/x.jsp</code>) opens that JSP, and an OGNL/EL result (<code>$&#123;urlErrori&#125;</code>)
  jumps to the owning action's property. A JSP path that doesn't exist under the web app, or an OGNL
  root that isn't a property of the action, is flagged with a warning squiggle.
</p>
<p>
  The same gesture on a <strong>library or JDK method</strong> — <code>list.add(…)</code>,
  <code>LOGGER.info(…)</code> — opens that library's source view and lands <strong>on the method
  itself</strong>. The receiver is typed against the project's classpath, so it works on anything your
  dependencies resolve to, and it chains: from inside one library view you can go on to the next.
</p>
<p>
  Ctrl+B on a <strong>library or JDK type</strong> (one with no project source) opens a
  <strong>decompiled stub</strong> generated from its bytecode — the type declaration plus every field
  and method signature, with a header noting it's decompiled (method bodies aren't stored in a class
  file). It's cached, so opening it again is instant. A decompiled stub is a read-only view and is not
  validated (it has no bodies, so validation would only report noise).
</p>
<h2>Find usages</h2>
<p>
  In a <code>.svelte</code> file, <kbd>Alt</kbd> + <kbd>Shift</kbd> + <kbd>F7</kbd> asks for the
  usages of <strong>the component itself</strong> — the imports and the <code>&lt;Foo /&gt;</code>
  tags. It is a separate key because it is the one subject <kbd>Alt</kbd> + <kbd>F7</kbd> cannot be
  pointed at: the file is the component, so there is no name written inside it to put a caret on.
</p>
<p>
  Put the caret on a class, method or field and press <kbd>Alt</kbd> + <kbd>F7</kbd> to list every
  place it's used across the project in a popover — pick one to jump to it. It answers once the index
  is warm.
</p>
<p>
  A field counts <strong>every</strong> read of it, whether or not the code wrote a receiver:
  <code>this.count</code>, <code>other.count</code>, <code>Config.MAX</code> and the bare
  <code>count</code> that means <code>this.count</code> are the same field. That last shape is the
  usual one — and for a <code>static final</code> constant it is often the only one. A local
  variable or parameter of the same name is that variable, not the field it hides, so a
  <code>setValue(int value)</code> does not report its own parameter as a use of
  <code>this.value</code>. A declaration is never a use of itself.
</p>
<h2>What a hover card says</h2>
<p>
  The signature, then the package, then — for a type that came out of a jar — the
  <strong>dependency it belongs to</strong>, as <code>groupId:artifactId:version</code>. The
  package says <em>which</em> type; the coordinate says <em>whose</em>, which on a long classpath
  is the question actually being asked: two jars declaring the same simple name, or a class from a
  starter nobody remembers adding.
</p>
<p>
  Below that, the documentation — from your own source, or from the library's
  <code>-sources.jar</code> when it has been downloaded.
</p>

<h2>Usage counts, and what nothing reaches</h2>
<p>
  Above a class, method or field, a line saying how many places use it. It is the same
  whole-project index find-usages reads, so it counts uses in files you have never opened, and
  pressing the line opens the very list it counted. Turn it off in
  <strong>Settings → Editor → Usage counts</strong>.
</p>
<p>
  A declaration nothing reaches has its <strong>name drawn faint</strong>. That claim is
  deliberately harder to earn than a count of zero, because a name faded wrongly is an invitation to
  delete working code. Zero uses is a fact about the index; unused is a fact about the program.
</p>
<p>
  So a declaration is one of <strong>three</strong> things, and only the first two are drawn at all:
</p>
<ul>
  <li><strong>Nothing outside the code reaches it.</strong> The count is the whole story, and a zero
    fades the name. This is the case the feature exists for.</li>
  <li><strong>Something might.</strong> It carries an annotation the engine does not recognise —
    <code>@Deprecated</code>, something of your own — so no claim either way is safe. The count is
    shown and nothing is faded: a deprecated method nobody calls is exactly what you were looking
    for.</li>
  <li><strong>Something does, by design.</strong> A <code>@Test</code> is run by JUnit, a
    <code>@Bean</code> built by Spring, a <code>@GetMapping</code> called by the dispatcher, a
    <code>@PrePersist</code> by the persistence provider, <code>main</code> by the JVM, an override
    through its supertype. For these a count of zero is not a finding, it is the wrong question —
    so <strong>nothing is drawn at all</strong>. A row saying “no usages” above every method of a
    test file is true and useless.</li>
</ul>
<p>
  The last of those still shows a count when there <em>is</em> one: a <code>@Bean</code> method
  called from another <code>@Bean</code> method in the same configuration is an ordinary call, and
  the number is worth having. Silence is only ever for a zero.
</p>
<p>
  <strong>A type whose members a framework calls is one itself</strong> — which is how a test class
  goes quiet without anybody guessing from its name. A class holding <code>@Test</code> methods is a
  class JUnit instantiates; the same reasoning covers a configuration full of <code>@Bean</code>
  factories and a controller full of mappings. A <code>TestUtils</code> with no test in it is an
  ordinary class, and if nothing uses it you are told.
</p>
<p>
  A constructor gets no count at all rather than a count of zero: its callers are <code>new</code>
  expressions, which the index keys by the type.
</p>
<p>
  What is left — a plain declaration, no annotation, no supertype, nothing calling it — is exactly
  the set <em>Safe delete</em> would agree to remove. One question, asked once, so the colour and
  the refactoring cannot disagree.
</p>

<h2>Call and type hierarchy</h2>
<p>
  <kbd>Ctrl</kbd> + <kbd>Shift</kbd> + <kbd>H</kbd> on a method opens its <strong>callers</strong>;
  <kbd>Ctrl</kbd> + <kbd>H</kbd> opens the <strong>type hierarchy</strong> of the class you are in —
  the caret may be anywhere inside it. Both land in the Hierarchy panel at the bottom, which has a
  direction chip to walk the other way: callees, or what a type is built on. The tree expands one
  level at a time, so a recursive chain is something you walk into as far as you care to.
</p>
<p>
  It is the same index find-usages reads, which is why the two agree — and why the callers of a
  method declared on an <strong>interface</strong> include the ones written against an
  implementation, and the other way round. That is the same family a rename carries.
</p>
<p>
  A caller row jumps to the <em>call</em> rather than to the head of the method containing it, and
  says <code>2×</code> when there is more than one call to the same thing inside it. A call inside a
  field initialiser or a static block has no method to name, so it is filed under its class, and
  that row is a leaf.
</p>
<p>
  Two honest limits. Calls into a <strong>dependency</strong> are not there: the index records an
  edge only when the callee is the project's own code, which is what keeps the list about your code.
  And <strong>overloads collapse</strong> — <code>process(String)</code> and
  <code>process(int)</code> are one row, because the index keys a method by its name. A supertype
  that lives in a jar does appear in a type hierarchy (a class that <code>extends HttpServlet</code>
  is not built on nothing) and can be expanded upward, but has no source to jump to.
</p>
<h2>Hover</h2>
<p>
  Rest the pointer on a class, method or field to see a card with what it is (a tag: class,
  interface, enum, method, field), its signature, and the type that <em>declares</em> it — the
  supertype, when you're hovering an inherited member. It answers from the project index, so it
  appears once the index is warm.
</p>
<p>
  A <strong>Javadoc is rendered, not printed</strong>. A doc comment is HTML — that is what the
  language says it is — so its headings are headings, its <code>&lt;pre&gt;</code> examples are code
  blocks that scroll sideways rather than wrap, its lists are lists, and
  <code>&amp;#064;</code> is an <code>@</code>. The inline forms read as what they name:
  <code>&lbrace;@link …&rbrace;</code> as the member it points at,
  <code>&lbrace;@code …&rbrace;</code> as code.
</p>
<p>
  <code>@param</code>, <code>@return</code> and <code>@throws</code> are lifted out of the prose
  into a labelled list, because they are a table; <code>@deprecated</code> is highlighted, because
  it is the one that changes what you do next. An <code>@Override</code> inside a code example
  stays in the example.
</p>
<p>
  Angle brackets that are not markup are left alone: <code>Vec&lt;T&gt;</code> and
  <code>Map&lt;K, V&gt;</code> are type parameters, not elements, and a card that ate them would be
  hiding the part you were reading for.
</p>
<p>
  Code blocks are <strong>syntax-coloured</strong>, with the same grammars a fenced block in a
  rendered <code>.md</code> gets: Java for a Javadoc, and whatever the file is for what a language
  server documents.
</p>
<p>
  A card longer than the space above the line <strong>scrolls</strong>, with the signature, the
  package and the coordinate staying put at the top of it. Move the pointer onto the card to
  scroll it.
</p>
<p>
  A <strong>library's</strong> documentation appears on the same card, from the same place its
  parameter names come from: the dependency's sources jar, or the JDK's own <code>src.zip</code>. A
  dependency whose sources are not downloaded shows the signature alone — a <code>.class</code>
  carries no comments — and “Download sources” on its decompiled view fills the cards in from then
  on. The signature itself is written as Java either way, generics and
  <code>throws</code> included: <code>&lt;X extends Throwable&gt; T orElseThrow(Supplier&lt;? extends
  X&gt;) throws X</code>.
</p>
<p>
  Hovering a <strong>variable</strong> — a local, a parameter, a loop variable, a
  <code>catch</code> parameter, a pattern variable — names its type, its
  <strong>fully-qualified</strong> type (which of the four <code>Order</code>s on the classpath this
  one is) and <strong>what that type is</strong>: class, interface, enum, record or annotation. A
  <code>var</code> or a Lombok <code>val</code> never shows as <code>var</code>: the card shows the
  type the compiler deduced, including the element type in
  <code>for (val row : rows)</code>.
</p>
<p>
  In a JSP, hovering a form field, an OGNL reference or a <code>*-validation.xml</code>
  <code>&lt;field&gt;</code> shows the <strong>type</strong> of the matching property on the bound
  action class, along with the action it belongs to.
</p>
