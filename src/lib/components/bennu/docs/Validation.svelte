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
  config XML are the files an analyzer understands; a <code>.dig</code>, TOML or SQL buffer is
  edited and highlighted but not checked. A language a <strong>server</strong> serves — Rust,
  TypeScript, Svelte — is checked by that server instead, and its results land in the same Problems
  panel.)
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
    class name in a declaration, <code>extends</code>, generics or <code>catch</code>).
    <strong>An annotation counts</strong>: <code>&#64;SpringBootApplication</code> with no import
    above it is the same "cannot find symbol", and it is the easiest one to leave behind, because
    the code around an annotation still reads correctly without it.
    <kbd>Alt</kbd> + <kbd>Enter</kbd> on the name offers the import.</li>
  <li><strong>Type incompatibility</strong> — an impossible cast (<code>(String) anInteger</code>),
    and an assignment or <code>return</code> whose value isn't of the declared type — including
    <code>String</code>/number mixups like <code>int x = "1";</code> or <code>int y = "1" + 1;</code>.
    Reference types are compared only between concrete classes, so it never second-guesses interface
    or generic code (boxing and widening are allowed). A <strong>fluent chain</strong> is checked
    like anything else — <code>Optional.ofNullable(repo.kind()).orElse(null)</code> returned as an
    <code>Integer</code> is caught — with one exception: a chain that is handed a <strong>lambda or
    method reference</strong> takes its type from that function, which Bennu doesn't type, so it is
    left to the compiler rather than guessed at.</li>
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
    methods/constructors with the same signature). Also a <strong>lambda parameter that shadows</strong>
    a name already in scope where the lambda is written — an enclosing lambda's parameter, the
    method's parameter, or a local declared before it. A field may be shadowed, and is not reported.</li>
  <li><strong>Unreachable code</strong> — a statement that can never run because the line before it
    always <code>return</code>s, <code>throw</code>s, <code>break</code>s or <code>continue</code>s.</li>
  <li><strong>Switch</strong> — a <code>switch</code> on a type it doesn't accept
    (<code>long</code>/<code>float</code>/<code>double</code>/<code>boolean</code>), and a
    <code>switch</code> <em>expression</em> arm that doesn't <code>yield</code> a value.</li>
  <li><strong>Case labels</strong> — a label that can't match the selector: a number or a string
    where the selector is an <strong>enum</strong> (an enum label has to be the unqualified name of a
    constant), a name that is no constant of that enum, and a literal of the wrong family on a
    <code>String</code> or boxed-integer selector.</li>
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
    sealed types, <code>var</code>, text blocks, switch arrows, lambdas, …). What
    <code>switch</code> accepts widened across releases, and each widening is caught where it is
    written: a <code>String</code> selector needs Java 7, <code>yield</code> needs 14, and type
    patterns, <code>when</code> guards and <code>case null</code> need 21. A <code>var</code>
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
  The compiler's own errors appear in the buffer too, marked <em>(build)</em> and carrying whatever
  javac said about them — the symbol it could not find, the type it required against the one it
  found. They describe the file as the compiler read it, so editing it clears them and live
  validation covers the file until the next build.
</p>

<h2>Data flow</h2>
<p>
  Three checks follow a <em>value</em> through a method rather than reading a declaration: a member
  reached on a local that is definitely <code>null</code>, a null check whose answer is already
  known, and a value assigned to a local and overwritten before anything reads it.
</p>
<p>
  The model is deliberately narrow. It reads a method's statements in order and <strong>forgets
  everything at the first branch</strong> — an <code>if</code>, a loop, a <code>try</code>, a
  <code>switch</code> — and it tracks <strong>locals only</strong>, never fields, since another
  method could change a field between two lines. So it misses more than it finds, and that is the
  trade: a flow analysis that is wrong accuses working code of throwing, and leaves the reader no
  way to see why except to reconstruct it in their head.
</p>

<h2>Turning a check down</h2>
<p>
  Under Project Configuration → <strong>Inspections</strong>, every check has a severity you can set
  to <strong>error</strong>, <strong>warning</strong>, <strong>weak</strong> or <strong>off</strong>.
  That is a policy over <em>kinds</em> — the right shape for "this project does not care about unused
  imports".
</p>
<p>
  For one place rather than one kind, the source says so:
  <code>&#64;SuppressWarnings("unused-import")</code> on the enclosing declaration, or a
  <code>// bennu:ignore unused-import</code> comment on the offending line or the one above it. A
  marker naming no code silences the line it governs. Javac's own vocabulary —
  <code>unused</code>, <code>fallthrough</code>, <code>all</code> — is honoured where it overlaps, so
  a legacy file that already carries it does not have to say the same thing twice.
</p>

<h2>Naming conventions</h2>
<p>
  A project can declare how its declarations are spelled, and have every name that breaks it
  flagged. It's <strong>off until you turn it on</strong>, per kind of declaration, under
  <strong>Project Configuration → Naming conventions</strong> — nothing is assumed about a project
  that never asked.
</p>
<p>
  It works for <strong>Java, TypeScript, JavaScript and Rust</strong>, with one difference worth
  knowing. Java is read by Bennu's own parser, so it sees every declaration — locals and parameters
  included. The others are read from the <strong>language server's outline</strong>, which lists
  types and their members and nothing else: locals and parameters are simply not in it, and those
  rows are greyed out for those languages rather than offered as rules that would never fire. Those
  languages also need their server installed for the check to have anything to work from.
</p>
<p>
  Pick a convention for each kind — types, methods, fields, constants, parameters, locals, type
  parameters, enum constants, package segments — or leave it at <code>any</code>, which checks
  nothing. <em>Use the standard convention</em> fills the column with what the language's community
  uses (for Java: <code>PascalCase</code> types, <code>camelCase</code> members,
  <code>UPPER_SNAKE_CASE</code> constants). The conventions are a fixed list rather than patterns
  you write, and that's what makes the fix possible: a pattern can refuse a name, a convention can
  <em>build</em> the right one.
</p>
<p>
  A violation is a <strong>weak warning</strong> — its own level, below errors and warnings, drawn
  faintly and grouped on its own in the Problems panel. A name that breaks a house style is true,
  but it isn't a defect, and a project adopting a convention gets one finding per offending
  declaration.
</p>
<p>
  <kbd>Alt</kbd> + <kbd>Enter</kbd> on the name offers <em>Rename to
  <code>theRightName</code></em>. For a Java local variable or parameter it renames straight away —
  those can't be referred to from outside their file, so the rename is exact. For anything a caller
  could also be using — a method, a field, a type, and <em>everything</em> in a language read
  through its server — it opens the rename preview with the name filled in, so you see every file it
  touches before it happens. A rename doesn't rewrite names inside JSP, OGNL or reflection strings,
  which is exactly why those fixes ask first.
</p>
<p>
  For more than one or two, don't visit them: the Command Palette has <em>Fix naming in file</em>
  and <em>Fix naming in project</em>. The review opens straight away and fills in as the plan is
  built — with progress, and a <strong>Stop</strong> that still hands you what it had — then shows
  what it would do, and lets you disagree with part of it. Nothing is written until you apply, and
  the whole fix is a single Undo afterwards. A name is refused when two names in a file would
  become the same name, or when the spelling it wants is already used there — renaming onto an
  existing name is how a bulk fix turns compiling code into two members with one signature — when
  the method overrides something declared in a dependency, whose name a jar fixes and we can't
  change with it, or when the file's bytes aren't valid in the project's declared encoding, which
  makes the editor and the index read it differently and every offset in it unreliable. Every
  refusal is listed with its reason.
</p>
<p>
  The review is meant to be argued with. <strong>Group</strong> the names by file — which for Java
  is by class — or by kind of declaration, or not at all. Switch off a <strong>kind</strong>
  wholesale to leave every local alone and rename only methods, untick a whole
  <strong>group</strong>, or untick a single name. <strong>Filter</strong> by name to find the ones
  you care about. The footer always counts what Apply will actually do, so anything you hide is
  something that will not be renamed — there is no state where a name is out of sight and still
  applied. The list is windowed, so a project-wide fix running to thousands of names opens and
  scrolls at the same speed as a small one.
</p>
<p>
  Some things are never reported: <strong>generated code</strong> (build output directories, and any
  file carrying a <code>@Generated</code> annotation or a “do not edit” banner), constructors (the
  name is the class's — the type is reported instead), <code>@Override</code> methods (the name
  belongs to the supertype), and platform-mandated names like <code>serialVersionUID</code>. Add
  your own exclusions as path globs under <em>Never check</em>.
</p>
<p>
  A project rarely has one convention, so a subtree can have its own. Under <em>Exceptions</em>, name
  one, give it path globs, and set the conventions that apply inside it — <strong>only</strong> the
  ones you name are replaced, so the rest of the rules still hold there. Test sources are the usual
  case: names like <code>test00_invalid_ragioneSociale</code> mix camelCase and snake_case on
  purpose, and an exception that sets <em>method</em> to <code>any</code> under
  <code>**/src/test/**</code> stops reporting them without giving up the type and constant rules the
  way <em>Never check</em> would. When two exceptions claim the same file, the later one wins.
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
  <strong>Errors</strong> node and a <strong>Warnings</strong> node at the top, then
  <strong>Weak warnings</strong> (style findings, such as a naming-convention violation) and the
  informational levels below them — each split by source (a JDK node, an Encoding node, and one node
  per file), so a file with both errors and warnings appears under both with just its rows of that
  severity. Every node is collapsible. It updates live
  for the file you're editing: as you fix a
  problem it disappears, and a newly-introduced one shows up — no need to re-run the whole-project
  validation to see the effect. That file's entry stays correct across the panel even after you
  switch to another file. Once you've run <em>Validate (no compile)</em> once, <strong>saving</strong>
  a file also silently refreshes the whole panel, so a fix that resolves an error in a
  <em>different</em> file (one that used what you changed) clears there too — again without re-running
  validation by hand.
</p>
<p>
  <strong>It is not a Java panel.</strong> Whatever a <strong>language server</strong> reports lands
  here too — rust-analyzer's <code>cargo check</code>, TypeScript's, Svelte's, Angular's on a
  template — including for files you have never opened, which is most of what a check produces. It
  arrives on its own: a server publishes as its check finishes, and the panel follows. Nothing has
  to be run by hand and nothing has to be armed, because those are not a project-wide sweep somebody
  opted into — they are what your build already says about your code.
</p>
<p>
  <strong>Only this project's files.</strong> A server reports on the whole crate graph it built,
  which for a <code>path</code> dependency means files in another repository — open geode and the
  panel would fill with the engine's warnings. They are real and they are not yours to fix from a
  window that does not have that project open, so they are left out. Open that project to see them;
  that is what opening a project means. The same rule drops a registry checkout and, when a session
  sits on an outer workspace, that workspace's other members.
</p>
<p>
  The four sources are kept apart on purpose. A polyglot repository can have a Java half that was
  validated and a Rust half that is being checked, and either replacing the other would mean a
  <code>cargo check</code> quietly erasing a validation depending on which finished last.
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
