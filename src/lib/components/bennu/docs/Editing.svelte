<!-- Bennu docs — Editing & navigation. -->
<h1>Editing &amp; navigation</h1>
<p class="doc-lead">
  The editor is a fast, syntax-highlighted view of your project's files with a tab strip,
  a symbol structure, code folding, completions, project-wide search and keyboard-first navigation.
  Java gets full semantic highlighting; XML/JSP, <code>.properties</code>, YAML, JSON, Markdown,
  HTML, CSS/SCSS, JavaScript, SQL, <strong>Rust</strong>, <strong>TOML</strong> and geode's
  <strong><code>.dig</code></strong> are highlighted too, so the whole stack reads cleanly.
</p>

<h2>Languages</h2>
<p>
  Three tiers, in descending order of what they know about the file:
</p>
<ul>
  <li><strong>Java</strong>, <strong>JSP</strong> and <strong><code>.dig</code></strong> parse with a
    real grammar: semantic highlighting and code folding. Java and JSP add navigation and
    index-backed completion; <code>.dig</code> completes from its own vocabulary (below).</li>
  <li><strong>HTML</strong>, <strong>JSON</strong> and <strong>Markdown</strong> highlight and fold.</li>
  <li><strong>Rust</strong>, <strong>TOML</strong>, <strong>RON</strong>, XML, YAML,
    <code>.properties</code>, CSS/SCSS/LESS, JavaScript/TypeScript, shell and SQL highlight.
    Colour only: navigation and completion in a Rust project want a language server, and until one
    is wired those actions are hidden rather than offered and silent.</li>
</ul>
<p>
  <strong>SQL</strong> is highlighted per <strong>dialect</strong>, because the engines disagree
  about string quoting: Oracle's <code>q'[…]'</code> and PostgreSQL's <code>$$ … $$</code> are each
  a broken string under the other's rules, and getting it wrong paints the rest of a file as one
  literal. Nothing inside a <code>.sql</code> file says which engine it targets, so it is a setting —
  <strong>Settings → Editor → SQL → Dialect</strong>. The default, <em>Portable</em>, uses the rules
  valid on both.
</p>

<h2>geode <code>.dig</code> scripts</h2>
<p>
  A <code>.dig</code> file is a mole program for <strong>geode</strong> — indentation-delimited, with
  a fixed set of host builtins. Bennu parses it with geode's own grammar, so highlighting and
  <strong>folding</strong> (a <code>fn</code>, <code>if</code>, <code>while</code>, <code>for</code>,
  <code>match</code> or <code>struct</code> body, and multi-line lists and maps) work from the syntax
  tree, and <kbd>Ctrl</kbd> + <kbd>/</kbd> toggles a <code>#</code> comment.
</p>
<p>
  Because the vocabulary is <strong>closed</strong>, completion and hover are answered locally — no
  index, no waiting. <strong>Completion</strong> offers the builtins, the reserved words, the
  namespaces, and the <code>fn</code> / <code>struct</code> / <code>let</code> names declared in the
  file; after <code>Crystal.</code> / <code>Tool.</code> / <code>Tick.</code> / <code>Speed.</code> /
  <code>Item.</code> it offers <strong>that namespace's members and nothing else</strong>. After a
  dot on anything else it offers the <strong>collection methods</strong> — both list and map, each
  labelled with its receiver, since without type inference both are true and picking one would be a
  guess. An <code>import</code> line is not completed: a geode module is a library unlocked in the
  shop, not a file on disk.
</p>
<p>
  <strong>Hover</strong> shows the signature and the full explanation, including the examples —
  the same help text the game shows, so <code>ripe_left()</code> explains the enormous-number answer
  and <code>block_size()</code> warns that a value above 1 also means a wall. A qualified member is
  looked up <em>with</em> its namespace, so <code>Speed.MAX_VALUE</code> and
  <code>Tick.MAX_VALUE</code> never show each other's text. Over a name the language doesn't own,
  hover stays silent.
</p>

<h2>Tabs</h2>
<ul>
  <li>Open a file from the <strong>Project</strong> tree or a <strong>Find in project</strong> hit — it joins the tab strip.</li>
  <li>Switch tabs with a click; the <strong>×</strong> closes one and a neighbour takes focus.</li>
</ul>

<h2>Structure</h2>
<p>
  The <strong>Structure</strong> tool (left rail) lists the active file's symbols — types, methods and
  fields — grouped by kind and filterable, sortable by position or name. Click a symbol to jump the
  editor to its declaration. Methods carrying an <code>@Override</code> annotation show an
  <strong>override marker</strong> (an up arrow), so the members that specialise a supertype stand
  out at a glance. The header carries <strong>Collapse all</strong> / <strong>Expand all</strong>
  chevrons to fold or unfold the whole tree at once, just like the Project panel.
</p>

<h2>Project tree</h2>
<p>
  The <strong>Project</strong> panel header carries quick actions: create a new file, locate the open
  file in the tree, collapse or expand the whole tree, and an options menu. Right-clicking a file or
  folder opens a context menu (Open · Copy path · Copy relative path · Reveal).
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

<h2>Code folding</h2>
<p>
  Braced blocks (classes, methods, blocks) and block comments fold from the gutter chevrons; the head
  line stays visible. In an indentation-delimited language like <code>.dig</code> the body folds from
  the end of its header line instead, with the same effect. Folding is computed live from the syntax
  tree — no indexing needed.
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

<h2>File health</h2>
<p>
  A badge in the editor's top-right corner shows the current file's error and warning counts (a green
  check when it's clean), mirroring the marks on the scrollbar overview.
</p>

<h2>Scrollbar overview</h2>
<p>
  The right-edge <strong>overview strip</strong> replaces the plain scrollbar: every error and warning
  is a coloured bar at its position in the file, so you see where the problems are at a glance. Hover
  the strip to preview the file at that spot, and click or drag it to jump. Toggle it in
  <strong>Settings → Editor → Scrollbar overview</strong>.
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

<h2>Completions</h2>
<p>
  Typing <code>.</code> after an expression offers member completions; press
  <kbd>Ctrl</kbd> + <kbd>Space</kbd> to request them explicitly. In Java, completions come from the
  project index and appear once it is warm. Edits re-index in the background as you type, so
  completion and go-to-definition track your changes without reopening the project. (In
  <code>.dig</code> they are answered locally — see <em>geode <code>.dig</code> scripts</em> above.)
</p>
<p>
  Typing a <strong>capitalised name</strong> (not after a dot) offers <strong>type-name
  completion</strong> — every class matching the prefix across the JDK, your dependencies and your
  project, with its package shown alongside (and a <em>(+N more)</em> hint when several packages
  declare the same simple name). Accepting one whose name maps to a <strong>single</strong> class
  also <strong>adds its import</strong> automatically (turn this off with Settings → Completion →
  <em>Auto-import on accept</em>). When the name is ambiguous — several packages — only the name is
  inserted; press <kbd>Alt</kbd> + <kbd>Enter</kbd> → <strong>Import '…'</strong> to pick the package.
</p>
<p>
  An <strong>overloaded</strong> method is offered <em>once per signature</em> — each entry showing
  its own parameters and return type — while a method that merely <strong>overrides</strong> an
  inherited one appears once. Inherited members are included; a <code>private</code> member of
  another class is not.
</p>

<h2>Saving</h2>
<p>
  <strong>Autosave is on by default</strong>: a modified file is written to disk automatically — a
  short moment after you stop typing, when you switch to another tab, and when the window loses focus
  (IntelliJ-style). You can still save explicitly with <kbd>Ctrl</kbd> + <kbd>S</kbd>. Turn autosave
  off in Settings → Editor → <strong>Autosave</strong> to save only on <kbd>Ctrl</kbd> + <kbd>S</kbd>;
  the choice persists across sessions.
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

<h2>Validation</h2>
<p>
  Java files are checked <strong>as you type</strong>, without compiling. Errors show as red
  squiggles, warnings as yellow, and everything is also listed in the Problems panel. (Java, JSP and
  config XML are the files an analyzer understands; a Rust, <code>.dig</code>, TOML or SQL buffer is
  edited and highlighted but not checked — in a Cargo project the checker is
  <strong>Check project</strong>, which runs <code>cargo check</code>.)
</p>
<p>
  <strong>Static imports are understood</strong>: a member you bring in with
  <code>import static …</code> and use unqualified (<code>PI</code>, <code>max(a, b)</code>) resolves
  to its type and isn't reported as an unknown symbol — while a name that <em>isn't</em> supplied by
  any static import is still caught.
</p>
<ul>
  <li><strong>Syntax errors</strong> — a malformed statement, a missing <code>;</code> or brace.</li>
  <li><strong>Not a statement</strong> — an expression Java won't accept as a statement, e.g.
    <code>list.clear;</code> (you forgot the call <code>()</code>) or <code>1 + 1;</code>.</li>
  <li><strong>Unknown method or field</strong> — a call or field access that doesn't exist on the
    receiver's type (found by inferring the receiver, so <code>s.lenght()</code> on a
    <code>String</code> is caught).</li>
  <li><strong>Wrong argument count</strong> — a method call or <code>new</code> whose number of
    arguments matches no overload (varargs are understood).</li>
  <li><strong>Wrong argument type</strong> — an argument that can't be passed to the parameter
    (<code>foo(1)</code> where <code>foo</code> takes a <code>String</code>). Checked only when a
    single overload is unambiguous, to avoid false positives.</li>
  <li><strong>Unresolved import</strong> — an <code>import</code> of a type that doesn't exist (a
    typo or a removed class). Needs the project classpath to be complete.</li>
  <li><strong>Unresolved type</strong> — a type name that doesn't resolve to any class (a typo'd
    class name in a declaration, <code>extends</code>, generics or <code>catch</code>).</li>
  <li><strong>Type incompatibility</strong> — an impossible cast (<code>(String) anInteger</code>),
    and an assignment or <code>return</code> whose value isn't of the declared type — including
    <code>String</code>/number mixups like <code>int x = "1";</code> or <code>int y = "1" + 1;</code>.
    Reference types are compared only between concrete classes, so it never second-guesses interface
    or generic code (boxing and widening are allowed).</li>
  <li><strong>Missing / wrong return</strong> — a non-<code>void</code> method that can finish without
    returning, a value returned from a <code>void</code> method or constructor, or a bare
    <code>return;</code> where a value is required.</li>
  <li><strong>Inheritance errors</strong> — extending a <code>final</code> class, a
    <code>record</code>, an <code>enum</code> or an interface; implementing a non-interface; a
    concrete class that leaves an inherited <code>abstract</code> method unimplemented.</li>
  <li><strong>Constructors</strong> — two methods or two constructors with the same signature, and a
    subclass constructor that must call <code>super(…)</code> because its superclass has no no-arg
    constructor.</li>
  <li><strong>Final</strong> — reassigning a <code>final</code> variable or field that already has an
    initial value, and overriding a <code>final</code> method inherited from a superclass. A
    <code>final</code> field left uninitialized (then assigned once, e.g. across <code>if</code>/<code>else</code>
    branches) is allowed.</li>
  <li><strong>Duplicate declarations</strong> — two fields, two method/constructor parameters, two
    local variables in one block, or two types with the same name in one scope (in addition to two
    methods/constructors with the same signature).</li>
  <li><strong>Unreachable code</strong> — a statement that can never run because the line before it
    always <code>return</code>s, <code>throw</code>s, <code>break</code>s or <code>continue</code>s.</li>
  <li><strong>Switch</strong> — a <code>switch</code> on a type it doesn't accept
    (<code>long</code>/<code>float</code>/<code>double</code>/<code>boolean</code>), and a
    <code>switch</code> <em>expression</em> arm that doesn't <code>yield</code> a value.</li>
  <li><strong>Lambdas</strong> — a lambda whose parameter count doesn't match its target functional
    interface (or a target that isn't a functional interface).</li>
  <li><strong>Declaration &amp; modifier errors</strong> — an <code>abstract</code> method in a
    concrete class, a <code>default</code> method outside an interface, illegal modifier
    combinations, a <code>record</code> that can't be abstract or declares instance fields, an
    <code>enum</code> constant that needs a constructor, and more.</li>
  <li><strong>Misplaced annotations</strong> — e.g. <code>@Override</code> on a field.</li>
  <li><strong>Lambda captures</strong> — modifying a captured local inside a lambda.</li>
  <li><strong>File name &amp; package</strong> — a <code>public</code> class whose name doesn't match
    the file, or a <code>package</code> that doesn't match the file's folder. Two
    <kbd>Alt</kbd>+<kbd>Enter</kbd> fixes are offered: <em>set the package</em> to match the folder,
    or <em>move the file</em> into the folder matching its declared package. The special
    <code>package-info.java</code> and <code>module-info.java</code> files are held to their
    restricted shape.</li>
  <li><strong>Java version</strong> — a feature newer than the project's target level (records,
    sealed types, <code>var</code>, text blocks, switch arrows, lambdas, …). A <code>var</code>
    back-ported by Lombok (imported from <code>lombok</code>) is allowed below Java 10.</li>
  <li><strong>Imports</strong> — unused or duplicate imports, and a redundant wildcard import
    (<code>import java.lang.*;</code> or a wildcard on the file's own package, both already in scope).</li>
  <li><strong>Generics (syntax)</strong> — generic array creation (<code>new List&lt;String&gt;[]</code>),
    instantiating a type parameter (<code>new T()</code>), generics in an <code>instanceof</code>
    (<code>x instanceof List&lt;String&gt;</code>) or a <code>catch</code> type, and <code>this</code>/<code>super</code>
    used in a <code>static</code> context.</li>
  <li><strong>Type-argument count</strong> — a generic type given the wrong number of type arguments
    (<code>List&lt;String, Integer&gt;</code>, <code>Map&lt;String&gt;</code>), checked against the type's
    declared parameters. The diamond <code>&lt;&gt;</code>, wildcards and raw types are always fine.</li>
  <li><strong>Erasure clash</strong> — two overloads that look distinct but collide after generic
    type erasure (<code>f(List&lt;String&gt;)</code> and <code>f(List&lt;Integer&gt;)</code>).</li>
  <li><strong>Duplicate interface</strong> — the same interface listed twice in an
    <code>implements</code>/<code>extends</code> clause, or once with two different type arguments.</li>
  <li><strong>Cyclic inheritance</strong> — a type that transitively extends or implements itself.</li>
  <li><strong>@Override overrides nothing</strong> — a method marked <code>@Override</code> whose name
    exists nowhere in its (fully known) supertype hierarchy — usually a signature typo.</li>
  <li><strong>super.method()</strong> — a <code>super.foo()</code> call whose method doesn't exist
    anywhere in the superclass hierarchy.</li>
  <li><strong>Exception handling</strong> — an unreachable <code>catch</code> (a type already caught
    by a clause above), a multi-<code>catch</code> that lists a type together with its supertype, and
    a try-with-resources whose resource type isn't <code>AutoCloseable</code>.</li>
  <li><strong>Enum switch exhaustiveness</strong> — a <code>switch</code> <em>expression</em> over an
    enum that doesn't cover every constant and has no <code>default</code> (it names the missing ones).</li>
  <li><strong>Constructor lookalike</strong> — a method named exactly like its class (a constructor
    written with a return type by mistake, which Java silently treats as an ordinary method).</li>
  <li><strong>Warnings</strong> — assigning a variable to itself, a constant division or modulo by
    zero, comparing strings with <code>==</code> (reference, not contents), <code>switch</code>
    fall-through (a colon-style <code>case</code> without <code>break</code>), a
    <code>return</code>/<code>break</code>/<code>continue</code> inside <code>finally</code> (it
    discards a pending exception or result), and a stray empty statement (<code>;</code>).</li>
</ul>
<p>
  The resolver-backed checks (unknown members, argument count, unresolved types, type
  compatibility, inheritance and lambda targets) lean on the standard library and dependencies, so
  they run once a JDK is available and stay silent about anything they can't resolve with certainty —
  they never report a false error.
</p>
<p>
  It's a best-effort check, so it complements <strong>Build</strong> (which runs the real compiler)
  rather than replacing it — more type checks arrive as the semantic engine grows.
</p>
<p>
  These checks normally run on the file you're editing, but you can run them over the <strong>whole
  project</strong> at once: in a Maven project the <strong>Build</strong> button is a split-button —
  open its chevron and
  pick <em>Validate (no compile)</em> (or make it the default so <kbd>Ctrl</kbd> + <kbd>F9</kbd> runs
  it). It validates every <code>.java</code> file without invoking a compiler and reports timing
  statistics — total time, average per file and the slowest file (with a fast/normal/slow verdict) —
  in the Build tool window, while every problem it finds appears in the <strong>Problems</strong>
  panel grouped by file. A build and a validation can't run at the same time.
</p>
<p>
  <strong>Errors decide the verdict; warnings never do.</strong> A run that ends with warnings only
  reads as <em>passed</em>, and the counts are coloured for what they are — red errors, yellow
  warnings, grey when there are none. Only a run with real errors is red.
</p>
<p>
  Validation runs across CPU cores, and each file's result is cached against the exact project types
  it depends on — so re-validating an unchanged project is instant, and after an edit only the
  changed file (and anything whose types it touched) is re-checked. The cache is warmed up in the
  background right after a project finishes indexing, so the first validation is already instant;
  turn that off under <strong>Settings → Java → Validate project on open</strong> to skip the
  background work. The sweep is a background citizen — it uses at most about half the CPU cores by
  default (so the editor, go-to and completion stay responsive); cap it under
  <strong>Settings → Java → Validation CPU threads</strong> (set 1 for single-threaded), and stop a
  running sweep with the <strong>Cancel</strong> button on the “Validating…” status in the Build panel.
</p>
<p>
  The <strong>Problems</strong> panel is a tree grouped <strong>by severity</strong> — an
  <strong>Errors</strong> node and a <strong>Warnings</strong> node at the top, each split by source
  (a JDK node, an Encoding node, and one node per file), so a file with both errors and warnings
  appears under both with just its rows of that severity. Every node is collapsible. It updates live
  for the file you're editing: as you fix a
  problem it disappears, and a newly-introduced one shows up — no need to re-run the whole-project
  validation to see the effect. That file's entry stays correct across the panel even after you
  switch to another file. Once you've run <em>Validate (no compile)</em> once, <strong>saving</strong>
  a file also silently refreshes the whole panel, so a fix that resolves an error in a
  <em>different</em> file (one that used what you changed) clears there too — again without re-running
  validation by hand.
</p>
<h3>Machine-generated expressions</h3>
<p>
  Everything that reasons about <em>types</em> — hover, the checks that compare one against another,
  completion after a dot — works by walking the expression it is looking at. Nesting is what that walk
  costs, and an expression's nesting is not bounded by what a person would write: a generated
  concatenation of a few thousand pieces (<code>"a" + "b" + …</code>, an unrolled query builder, a
  generated messages class) nests one level per piece.
</p>
<p>
  Past about <strong>128 levels</strong> Bennu stops descending and answers <em>unknown</em> for that
  expression. In practice that means a hover over it says nothing and the type-dependent checks skip
  it — <strong>only there</strong>, in that one expression. The syntax checks, the outline, find
  usages and go-to are unaffected, and every other expression in the file types normally. Hand-written
  code never reaches the limit; a long fluent chain is tens of levels, not hundreds.
</p>

<h2>Generated members</h2>
<p>
  Plenty of Java members exist at compile time and nowhere in the source. Bennu models them, so
  completion, hover, find-usages and the checks treat them like any declaration:
</p>
<ul>
  <li><strong>Records</strong> — an accessor per component (<code>p.x()</code>, named after the
    component, not <code>getX()</code>), the backing fields, the canonical constructor, and
    <code>toString</code> / <code>equals</code> / <code>hashCode</code>. A member the record writes
    itself always wins.</li>
  <li><strong>Lombok</strong> — <code>@Getter</code> / <code>@Setter</code> / <code>@Data</code> /
    <code>@Value</code> accessors, <code>@With</code> copy-methods, the
    <code>@Slf4j</code> <code>log</code> field, the
    constructor <code>@AllArgsConstructor</code> / <code>@RequiredArgsConstructor</code> generates
    (including on an enum with valued constants), and <code>@UtilityClass</code> — which makes every
    member <code>static</code> and the class <code>final</code>.</li>
</ul>
<p>
  Lombok's members are honoured only when the file actually <strong>imports</strong> Lombok, since
  that is what makes the annotation mean anything — your own <code>@Data</code> in another package
  generates nothing. A record's members need no such gate: they come from the language.
</p>
<p>
  <code>@Accessors</code> is honoured too, at class or field level: <code>fluent = true</code> names
  both accessors after the field (<code>o.customer()</code> reads, <code>o.customer("x")</code>
  writes), and <code>chain = true</code> — which <code>fluent</code> turns on by itself — makes the
  setter return the object so calls chain. A field's own <code>@Accessors</code> overrides the
  class's. The <code>prefix</code> element is not read yet.
</p>
<p>
  <code>AccessLevel</code> is honoured on <code>@Getter</code> / <code>@Setter</code>, at class or
  field level: <code>@Setter(AccessLevel.PACKAGE)</code> generates a package-private setter and is
  treated as one, and <code>AccessLevel.NONE</code> generates nothing at all — so no accessor is
  offered for a field that has switched it off. The generated <strong>constructors</strong> carry
  their <code>access = AccessLevel.…</code> the same way, and take the parameters Lombok actually
  gives them — <code>@RequiredArgsConstructor</code> takes the <code>final</code> fields that aren't
  already assigned, plus the <code>@NonNull</code> ones.
</p>
<p>
  A primitive <code>boolean</code> field whose name <em>already</em> begins with <code>is</code> keeps
  it rather than getting a second one, exactly as Lombok does: <code>isRunning</code> gives
  <code>isRunning()</code> and <code>setRunning(…)</code>, and <code>is_attivo</code> gives
  <code>is_attivo()</code>. The rule applies whenever what follows <code>is</code> is not a lowercase
  letter, so a field named <code>isattivo</code> does get the prefix (<code>isIsattivo()</code>). A
  <code>Boolean</code> wrapper is a plain <code>getX</code>.
</p>
<p>
  One limitation, and it is deliberate: <strong>go-to on a generated member</strong> has nothing to
  open, since there is no name in the source to jump to. Go-to on the backing <em>field</em> (or a
  record's component) works.
</p>

<h2>Find</h2>
<p>
  <kbd>Ctrl</kbd> + <kbd>F</kbd> searches the current file. <kbd>Ctrl</kbd> + <kbd>Shift</kbd> +
  <kbd>F</kbd> opens <strong>Find in project</strong> — a backend-powered search across the whole
  project with <strong>Match case</strong>, <strong>Whole word</strong> and <strong>Regex</strong>
  toggles, grouping hits by file with the match highlighted. Results stream in as the scan finds
  them, so a large project fills the list instead of making you wait for it.
</p>
<p>
  The selected hit is shown <strong>in context</strong> beside the list — the lines around it, with
  the match highlighted — which is what tells four identical-looking lines apart without opening
  four files. ↑/↓ move the selection (and the preview follows), <kbd>Enter</kbd> opens the hit.
  A <strong>file mask</strong> narrows what is listed: <code>*.java</code>, or several at once as
  <code>*.jsp, *.tag</code>. If a word is <strong>selected</strong> in the editor, it pre-fills the
  search field (both here and in Find-in-file).
</p>
<p>
  The <strong>📦 toggle</strong> also searches inside the <strong>dependency jars</strong> — their
  XML, schemas, tag libraries and property files — which is how you find which artifact declares the
  interceptor or the bean you are looking at. Those hits arrive after the project's own, and opening
  one extracts it read-only. It is off by default and per-search rather than a setting: every
  candidate entry has to be decompressed to be read, so it is a cost you take for the question you
  are asking now.
</p>

<h2>Go to class / file / symbol</h2>
<p>
  One navigator over three lists, with a tab each: <strong>Classes</strong>, <strong>Files</strong>
  and <strong>Symbols</strong> (every method and field the project declares), plus an
  <strong>All</strong> tab that searches them together and keeps each one's best few under its own
  heading. <kbd>Ctrl</kbd> + <kbd>N</kbd>, <kbd>Ctrl</kbd> + <kbd>Shift</kbd> + <kbd>N</kbd> and
  <kbd>Ctrl</kbd> + <kbd>Shift</kbd> + <kbd>Y</kbd> open it on Classes, Files and Symbols
  respectively; <kbd>Tab</kbd> moves between them without reopening.
</p>
<p>
  Matching is by <strong>subsequence</strong>, not substring: <code>agpo</code> finds
  <code>AGGIORNAMENTO/POS</code> because the letters appear in order, and the characters that
  matched are lit in the row so a loose match is legible rather than mysterious. Several terms are
  each matched independently, so <code>agg pos</code> works on a path that separates them. Results
  rank on where the hit lands — the start of a word beats the middle, a run beats a scattered match,
  a short name beats a long one.
</p>
<p>
  Three directives narrow a search further: <code>in:dao</code> (the path contains it),
  <code>ext:java</code> (that extension) and <code>sort:new</code> (most recently modified first).
  ↑/↓ move, <kbd>Enter</kbd> opens — a class or symbol jumps straight to its declaration line. A word
  selected in the editor pre-fills the field. Files work in any project; Classes and Symbols read the
  Java index, so they aren't offered in a Cargo one.
</p>

<h3>Reaching what is inside the dependencies</h3>
<p>
  <strong>Search the dependencies too</strong> (Settings → Java) adds two more tabs:
  <strong>Library classes</strong> and <strong>Library files</strong> — everything on the
  dependency classpath that is nowhere in the tree. The framework annotation whose package you are
  trying to remember, the <code>struts-default.xml</code> that declares the interceptor stack, the
  schema an XML file is validated against. Each row says which <strong>artifact</strong> it came
  from, because a classpath is where four versions of the same name live.
</p>
<p>
  Opening a library <em>class</em> shows its source the same way a stack-trace frame does: the real
  <code>.java</code> when the JDK ships sources or a <code>-sources.jar</code> has been downloaded,
  otherwise the decompiled stub. A library <em>file</em> is extracted from the jar and opened
  read-only, keeping its extension — so an XML still reads as XML.
</p>
<p>
  These two are searched as you type rather than listed: a classpath is hundreds of thousands of
  entries, and nothing is fetched until there is a query. The first search after opening a project
  spends a moment reading the jars, and is instant after that.
</p>

<h2>Mojibake check</h2>
<p>
  <strong>Check file for mojibake</strong> (Command Palette) scans the open file for text that was
  UTF-8 but got read as Windows-1252 — the classic <code>Ã©</code> for <code>é</code> or
  <code>â€™</code> for <code>'</code>. Each hit is squiggled with a one-click
  <strong>Replace with «…»</strong> quick-fix, and a summary tells you how many were found.
  Detection is exact (a table of real corruption sequences), so clean accented text is never flagged.
</p>

<h2>Find usages</h2>
<p>
  Put the caret on a class, method or field and press <kbd>Alt</kbd> + <kbd>F7</kbd> to list every
  place it's used across the project in a popover — pick one to jump to it. It answers once the index
  is warm.
</p>

<h2>Save</h2>
<p>
  <kbd>Ctrl</kbd> + <kbd>S</kbd> writes the current file to disk in the project's encoding. Rename
  applies and saves its edits the same way.
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

<h2>Hover</h2>
<p>
  Rest the pointer on a class, method or field to see a card with what it is (a tag: class,
  interface, enum, method, field), its signature, and the type that <em>declares</em> it — the
  supertype, when you're hovering an inherited member. It answers from the project index, so it
  appears once the index is warm.
</p>
<p>
  A <strong>Javadoc</strong> on a project declaration is read rather than dumped: the prose comes
  first, then <code>@param</code>, <code>@return</code> and <code>@throws</code> as a labelled list,
  with <code>&lbrace;@link …&rbrace;</code> shown as what it names and <code>@deprecated</code>
  highlighted.
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

<h2>Right-click menu</h2>
<p>
  Right-clicking in the editor opens a context menu with the clipboard actions (Cut · Copy · Paste)
  and the semantic ones — <strong>Go to declaration</strong>, <strong>Find usages</strong>,
  <strong>Rename</strong>, <strong>Generate</strong> and <strong>Save</strong>. The semantic actions
  act on the symbol <strong>under the pointer</strong> — right-clicking moves the caret there first.
</p>

<h2>Intentions</h2>
<p>
  <kbd>Alt</kbd> + <kbd>Enter</kbd> opens the <strong>intentions</strong> popup at the caret — a
  keyboard-driven list of the context actions available there (↑/↓ to move, <kbd>Enter</kbd> to
  apply, <kbd>Esc</kbd> to dismiss). It's the entry point to the generator flows and to quick-fixes.
</p>
<p>
  With the caret on a <strong>type that isn't imported</strong>, the popup offers
  <strong>Import '…'</strong> — it adds the <code>import</code> line for you (placed after the package
  declaration and sorted among the existing imports). When more than one class shares that name, each
  candidate is listed as its own entry, so you pick the package you meant; a type in the same package,
  in <code>java.lang</code>, or already covered by a wildcard import isn't offered (it needs none).
</p>
<p>
  More quick-fixes live here too. With the caret inside a logging call whose message is built by
  string concatenation — <code>logger.info("user " + id + " logged in")</code> — the popup offers
  <strong>Replace concatenation with parameterized logging</strong>, rewriting it to the form the
  logging APIs prefer: <code>logger.info("user &lbrace;&rbrace; logged in", id)</code> (a trailing
  exception argument is kept last). On a <code>x.equals("literal")</code> call it offers
  <strong>Flip to null-safe equals</strong> — <code>"literal".equals(x)</code>, which never throws
  when <code>x</code> is null. And a family of one-click <strong>simplifications</strong>:
  <code>list.size() == 0</code> → <code>list.isEmpty()</code>, <code>flag == true</code> →
  <code>flag</code>, <code>!(a == b)</code> → <code>a != b</code>.
</p>

<h2>Rename</h2>
<p>
  Put the caret on a symbol and press <kbd>Shift</kbd> + <kbd>F6</kbd> to rename it across the
  project. A <strong>preview</strong> lists every edit grouped by file before anything is written —
  confirm to apply (through the editor, so a single <kbd>Ctrl</kbd> + <kbd>Z</kbd> undoes the whole
  rename). What gets rewritten depends on what the caret is on:
</p>
<ul>
  <li>a <strong>local variable</strong> or <strong>parameter</strong> — scope-exact, in that method only, never a same-named variable elsewhere or a field of the same name;</li>
  <li>a <strong>method</strong> or <strong>field</strong> — its declaration and every use across the project;</li>
  <li>a <strong>class</strong> or <strong>interface</strong> — its declaration, references, <code>import</code> statements, and the matching Spring <code>&lt;bean class="…"&gt;</code> entries. A Struts <code>&lt;action class="…"&gt;</code> names a bean id, not the class, so it is left untouched.</li>
</ul>
<p>
  Edits that can't be pinned down exactly — an overloaded method's call sites, for instance — are
  marked for review in the preview rather than applied silently. It answers once the index is warm.
  OGNL and JSP references are not rewritten yet.
</p>

<h2>Spelling</h2>
<p>
  Opt-in per project (Project Configuration → <strong>Spelling</strong>): after downloading the
  English + Italian dictionaries, Bennu checks your <strong>declared names</strong> — split by
  camelCase, snake_case and kebab-case — and your <strong>comments</strong>. A misspelled word is
  underlined as a hint; <kbd>Alt</kbd> + <kbd>Enter</kbd> (or the lint action) offers to replace it
  with a suggestion or <strong>add it to a project or global dictionary</strong>. Common programming
  abbreviations are allow-listed, so it stays quiet on the usual jargon.
</p>

<h2>Generate</h2>
<p>
  <kbd>Alt</kbd> + <kbd>Insert</kbd> opens the <strong>Generate</strong> dialog — build a constructor,
  getters, setters or both from the active class's fields. Pick a mode, tick the fields to include,
  choose fluent or plain setters and camelCase or snake_case accessors; a live preview shows the code
  and <kbd>Ctrl</kbd> + <kbd>Enter</kbd> inserts it at the caret.
</p>

<h2>Indentation</h2>
<p>
  The footer shows the active indentation as <em>Spaces: N</em> or <em>Tab Size: N</em>. Click it (or
  focus it and press <kbd>↑</kbd>/<kbd>↓</kbd>) to switch between <strong>spaces and tabs</strong> and
  pick the <strong>tab width</strong> (2 / 4 / 8). The change applies to the open editor immediately.
</p>

<h2>JSP &amp; Struts navigation</h2>
<p>
  JSP files are highlighted by a dedicated grammar — namespaced taglib tags (<code>&lt;s:iterator&gt;</code>,
  <code>&lt;c:if&gt;</code>), scriptlets, EL <code>$&lbrace;…&rbrace;</code> and OGNL <code>%&lbrace;…&rbrace;</code> all colour
  correctly, and the <strong>inside</strong> of an EL/OGNL expression is tokenized too —
  identifiers, property accesses, strings, numbers, operators and keywords each get their own
  colour instead of one flat block. <kbd>Ctrl</kbd> + <kbd>B</kbd> (or <kbd>Ctrl</kbd> + click) on an
  <code>action="…"</code> reference jumps to the Struts <code>&lt;action&gt;</code> config, its view
  JSP, or the action class. <kbd>Alt</kbd> + <kbd>F7</kbd> on an action reference lists every JSP
  that uses it. The same keys work on a <strong>page-scoped JSP variable</strong> — a
  <code>&lt;c:set var="x"&gt;</code>, <code>&lt;s:set var="x"&gt;</code>,
  <code>&lt;c:forEach var="x"&gt;</code> or <code>&lt;s:iterator var="x"&gt;</code> and its
  <code>$&lbrace;x&rbrace;</code> / <code>%&lbrace;x&rbrace;</code> references: <kbd>Ctrl</kbd> +
  <kbd>B</kbd> on a reference jumps to where the variable is set, and <kbd>Alt</kbd> +
  <kbd>F7</kbd> lists every reference in the page (it's page-scoped, so all in the same file).
  <kbd>Ctrl</kbd> + <kbd>B</kbd> (or <kbd>Ctrl</kbd> + click) on a JSP <strong>include path</strong> —
  a <code>&lt;%@ include file="…"&gt;</code> directive, <code>&lt;jsp:include page="…"&gt;</code> or
  <code>&lt;s:include value="…"&gt;</code> — opens the referenced JSP.
  An absolute action reference that resolves to nothing gets a
  <strong>warning squiggle</strong> — a wildcard or runtime (<code>$&lbrace;…&rbrace;</code>/<code>%&lbrace;…&rbrace;</code>)
  reference never does. A static <strong>include</strong> whose target file doesn't exist
  (<code>&lt;%@ include file="…"&gt;</code>, <code>&lt;jsp:include page="…"&gt;</code>,
  <code>&lt;s:include value="…"&gt;</code>, <code>&lt;c:import url="…"&gt;</code>) is flagged the
  same way — a computed or <code>http(s)://</code> reference never is. Inline
  <code>&lt;script&gt;</code> and <code>&lt;style&gt;</code> blocks are highlighted as JavaScript and CSS.
</p>
<p>
  While editing a JSP, the editor toolbar shows an <strong>Insert tag</strong> menu that drops a
  ready-made JSTL / Struts snippet at the caret — <code>&lt;c:set&gt;</code>, <code>&lt;s:set&gt;</code>,
  <code>&lt;s:property&gt;</code>, <code>&lt;s:iterator&gt;</code>, <code>&lt;c:forEach&gt;</code>,
  <code>&lt;s:if&gt;</code> / <code>&lt;c:if&gt;</code>, <code>&lt;s:url&gt;</code>, <code>&lt;s:text&gt;</code> and
  <code>&lt;s:textfield&gt;</code> — with placeholder attributes you overtype.
</p>
<p>
  A <strong>form field</strong> in a JSP — the <code>name="…"</code> of an <code>&lt;s:textfield&gt;</code>,
  <code>&lt;input&gt;</code>, <code>&lt;s:select&gt;</code>, … inside a form — is understood as a property of
  the form's <strong>action class</strong>. <kbd>Ctrl</kbd> + <kbd>B</kbd> (or <kbd>Ctrl</kbd> + click)
  on it jumps to the matching <code>get</code>/<code>set</code>/<code>is</code> accessor in the action
  Java; and a field whose name is <strong>not</strong> a property of the action gets a
  <strong>warning squiggle</strong> (a likely typo — “this parameter doesn't exist on the action”).
  The check only fires when the action resolves to a project class whose properties are known, so an
  unresolved action never produces a false warning. The same works from a
  <code>&lt;field name="…"&gt;</code> inside a <code>*-validation.xml</code> — go-to jumps to the
  bound action's property, and an unknown field name is flagged the same way. Properties inherited
  from a project <code>BaseAction</code> are resolved up the <code>extends</code> chain, so they are
  never mis-flagged.
</p>
<p>
  For a <strong>view JSP</strong> with no form — just OGNL (<code>%&lbrace;customer&rbrace;</code>,
  <code>&lt;s:property value="…"/&gt;</code>) — the editor works out which action renders it from the
  Struts result mappings (the reverse of action → view). When exactly one action maps to the page it's
  used automatically; when several do (or you want to override), an <strong>action picker</strong> in
  the toolbar lets you pin one, remembered per file. The bound action drives <kbd>Ctrl</kbd> +
  <kbd>B</kbd> on an OGNL root and its “unknown property” warning. Only plain
  <code>%&lbrace;…&rbrace;</code> value-stack roots are checked — EL <code>$&lbrace;…&rbrace;</code>
  scoped attributes and <code>#</code>-prefixed context / iterator variables are left alone.
</p>
<p>
  This follows <strong>includes</strong>: an included fragment (<code>.jspf</code>) that a view page
  pulls in has no action of its own, so it <strong>inherits</strong> the action(s) of the page(s) that
  include it (transitively). So the picker, the go-to and the “unknown property” warning all work on a
  child fragment too — its fields (even those that belong to a form declared in the parent page) and
  its OGNL are checked against the parent view's action.
</p>

<h2>MyBatis mappers</h2>
<p>
  Inside a mapper <code>.xml</code>, <kbd>Ctrl</kbd> + <kbd>B</kbd> (or <kbd>Ctrl</kbd> + click) follows
  the piece under the caret: a statement <code>id="…"</code> jumps to the matching method on the
  mapper interface (<code>.java</code>); the mapper <code>namespace="…"</code> opens that interface;
  an <code>&lt;include refid="…"&gt;</code> jumps to the <code>&lt;sql&gt;</code> fragment it pulls in;
  and a statement's <code>resultMap="…"</code> jumps to the <code>&lt;resultMap&gt;</code> it uses.
  Fragment references within the same file resolve instantly (no index needed).
</p>

<h2>Struts validation files</h2>
<p>
  On a project that uses <strong>Struts</strong>, a Java <strong>action class</strong> gets a
  <strong>Validation</strong> button on the toolbar
  (also in the Command Palette): it creates the class's <code>&lt;Class&gt;-validation.xml</code>
  next to it — following the Struts naming convention — from a proper DTD-headed skeleton if it
  doesn't exist yet, then opens it. If it already exists, it just opens it.
</p>
<p>
  On a <code>&lt;Action&gt;-validation.xml</code> the toolbar shows <strong>Validators</strong>,
  which opens the <strong>chain builder</strong>. Pick a field (the action's writable properties are
  offered as chips) and stack an ordered <strong>chain</strong> of validators on it — add, remove
  and reorder them, each with its own parameters, message and <strong>short-circuit</strong> flag
  (stop the chain on first failure). Validator types and their parameters come from the built-in
  Struts catalog; a live preview shows the exact XML. <strong>Add to file</strong> appends the chain
  into the document — creating the <code>&lt;field&gt;</code> or growing an existing one — so you
  never place a caret by hand.
</p>

<h2>The index</h2>
<p>
  Completion, go-to-definition, find-usages, rename, hover and Go-to-Class all answer from a
  <strong>semantic index</strong> Bennu builds in the background when a project opens. The footer shows
  its progress and reads <em>Indexed · N types</em> once it's warm.
</p>
<p>
  The <strong>Index inspector</strong> (Command Palette → <em>Index inspector…</em>) browses what the
  index holds — types, members, jars, JDK, beans, actions and relations — with a filter and jump-to.
  If something looks stale or a class you know exists isn't turning up, press <strong>Rebuild</strong>
  there (or run <em>Rebuild index</em> from the palette) to invalidate the index and recompute it from
  scratch. This is a pure re-scan of the sources on disk — it doesn't compile the project (that's
  <kbd>Ctrl</kbd> + <kbd>F9</kbd>).
</p>

<div class="callout">
  Everything is reachable from the keyboard. The <strong>Command Palette</strong>
  (<kbd>Ctrl</kbd> + <kbd>K</kbd>) lists the editor and tool-window actions; the tool windows toggle
  with <kbd>Alt</kbd> + <kbd>1</kbd> / <kbd>2</kbd> (Project · Structure), <kbd>Alt</kbd> +
  <kbd>0</kbd> / <kbd>6</kbd> / <kbd>7</kbd> / <kbd>F12</kbd> (Build · Problems · TODO · Terminal), and
  <kbd>Alt</kbd> + <kbd>8</kbd> (Maven). Build the project with
  <kbd>Ctrl</kbd> + <kbd>F9</kbd> and run it with <kbd>Shift</kbd> + <kbd>F10</kbd>. In a Cargo
  project the Java-only tools and Run are hidden, and <kbd>Ctrl</kbd> + <kbd>F9</kbd> runs
  <code>cargo check</code>.
</div>
