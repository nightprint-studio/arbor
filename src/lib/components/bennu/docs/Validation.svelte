<!-- Bennu docs — the diagnostics Bennu produces itself, without a compiler. -->
<h1>Validation &amp; problems</h1>
<p class="doc-lead">
  The squiggles that appear while you type, before anything is compiled — what each tier checks,
  and what it deliberately stays quiet about.
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
  <strong>Settings → Java → Validation CPU threads</strong> (set 1 for single-threaded) — its sibling <strong>Indexing CPU threads</strong> caps the index build and the reference walk the same way, and is serial by default — and stop a
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
