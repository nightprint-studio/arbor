<script lang="ts">
  /**
   * Validation and problems: what Bennu checks while you type, what it keeps quiet about, turning checks down, naming
   * conventions, validating a whole project, and the Problems panel.
   */
  import Callout from '$lib/components/shared/ui/Callout.svelte';
  import { highlightCode } from '$lib/utils/highlight';
</script>

<span class="eyebrow">Editor</span>
<h1>Validation &amp; problems</h1>

<p class="doc-lead">
  The squiggles that appear while you type, before anything is compiled — what each check looks for, and what it deliberately stays quiet about.
</p>

<h2>Checked as you type</h2>
<p>
  Java files are checked <strong>as you type</strong>, without compiling: errors are red squiggles, warnings yellow, and everything is listed in the
  Problems panel.
</p>
<Callout variant="info" title="Which files are checked, and by whom">
  Java, JSP and configuration XML are the files an analyzer understands; a <code>.dig</code>, TOML or SQL buffer is highlighted but not checked. A
  language a <strong>server</strong> serves — Rust, TypeScript, Svelte — is checked by that server, and its results land in the same Problems panel.
</Callout>
<p>
  It is a best-effort check that complements <strong>Build</strong>, which runs the real compiler. The compiler's own errors appear in the buffer too,
  marked <em>(build)</em> with what javac said — the symbol it could not find, the type it required against the one it found. They describe the file as
  the compiler read it, so editing it clears them and live validation covers the file until the next build.
</p>

<h2>The checks</h2>
<h3>What the compiler would refuse</h3>
<ul class="prop-list">
  <li><strong>Syntax errors</strong>a malformed statement, a missing <code>;</code> or brace</li>
  <li><strong>Not a statement</strong><code>list.clear;</code> — the call's <code>()</code> forgotten — or <code>1 + 1;</code></li>
  <li><strong>Unknown method or field</strong>on the receiver's inferred type, so <code>s.lenght()</code> on a <code>String</code> is caught</li>
  <li><strong>Wrong argument count</strong>a call or <code>new</code> matching no overload; varargs understood</li>
  <li><strong>Wrong argument type</strong><code>foo(1)</code> where <code>foo</code> takes a <code>String</code> — only when a single overload is unambiguous</li>
  <li><strong>Unresolved import</strong>a type that does not exist; needs the classpath complete</li>
  <li><strong>Unresolved type</strong>in a declaration, <code>extends</code>, generics or <code>catch</code> — an annotation counts, and so does a nested type through its outer (below)</li>
  <li><strong>Type incompatibility</strong>an impossible cast, or a value of the wrong type assigned or returned — <code>int x = "1";</code>, <code>int y = "1" + 1;</code></li>
  <li><strong>Missing or wrong return</strong>a non-<code>void</code> method that can finish without returning, a value from a <code>void</code> method, a bare <code>return;</code> where a value is needed</li>
  <li><strong>Inheritance</strong>extending a <code>final</code> class, a record, an enum or an interface; implementing a non-interface; leaving an inherited <code>abstract</code> method unimplemented</li>
  <li><strong>Constructors</strong>two with one signature, and a subclass constructor that must call <code>super(…)</code> because the superclass has no no-arg one</li>
  <li><strong>Final</strong>reassigning a <code>final</code> that already has a value, or overriding a <code>final</code> method; a <code>final</code> field assigned once across <code>if</code>/<code>else</code> is fine</li>
  <li><strong>Duplicates</strong>two fields, parameters, locals in a block or types in a scope with one name — and a lambda parameter shadowing a name in scope (a field may be shadowed)</li>
  <li><strong>Unreachable code</strong>after a line that always returns, throws, breaks or continues</li>
  <li><strong>Switch</strong>on <code>long</code>, <code>float</code>, <code>double</code> or <code>boolean</code>, and a switch <em>expression</em> arm that yields nothing</li>
  <li><strong>Case labels</strong>a number or string for an enum selector, a name that is no constant of that enum, a literal of the wrong family on a <code>String</code> or boxed-integer selector</li>
  <li><strong>Lambdas</strong>a parameter count not matching the functional interface, or a target that is not one; modifying a captured local inside</li>
  <li><strong>Declarations and modifiers</strong>an <code>abstract</code> method in a concrete class, <code>default</code> outside an interface, illegal combinations, an abstract record or one with instance fields, an enum constant needing a constructor</li>
  <li><strong>Misplaced annotations</strong><code>@Override</code> on a field</li>
  <li><strong>File name and package</strong>a <code>public</code> class not matching its file, or a <code>package</code> not matching its folder — with <kbd>Alt</kbd> + <kbd>Enter</kbd> to set the package or move the file; <code>package-info.java</code> and <code>module-info.java</code> held to their shapes</li>
  <li><strong>Java version</strong>a feature newer than the target level — records, sealed types, <code>var</code>, text blocks, switch arrows, lambdas; a <code>String</code> selector needs 7, <code>yield</code> 14, type patterns, <code>when</code> guards and <code>case null</code> 21; a Lombok <code>var</code> is allowed below 10</li>
  <li><strong>Generics</strong><code>new List&lt;String&gt;[]</code>, <code>new T()</code>, generics in <code>instanceof</code> or <code>catch</code>, <code>this</code>/<code>super</code> in a static context</li>
  <li><strong>Type arguments</strong>the wrong count — <code>List&lt;String, Integer&gt;</code>, <code>Map&lt;String&gt;</code>; the diamond, wildcards and raw types are fine</li>
  <li><strong>Erasure clash</strong><code>f(List&lt;String&gt;)</code> beside <code>f(List&lt;Integer&gt;)</code></li>
  <li><strong>Duplicate interface</strong>listed twice, or twice with different type arguments</li>
  <li><strong>Cyclic inheritance</strong>a type that extends or implements itself, transitively</li>
  <li><strong><code>@Override</code> overriding nothing</strong>a name found nowhere in a fully known hierarchy — usually a signature typo</li>
  <li><strong><code>super.method()</code></strong>calling a method no superclass has</li>
  <li><strong>Exceptions</strong>an unreachable <code>catch</code>, a multi-catch listing a type with its supertype, a try-with-resources resource that is not <code>AutoCloseable</code></li>
  <li><strong>Enum switch exhaustiveness</strong>a switch expression over an enum missing constants and a <code>default</code> — naming the missing ones</li>
</ul>

<h3>Warnings</h3>
<ul class="prop-list">
  <li><strong>Imports</strong>unused or duplicate, and a redundant wildcard — <code>import java.lang.*;</code>, or one on the file's own package</li>
  <li><strong>Constructor lookalike</strong>a method named like its class — a constructor written with a return type, which Java silently treats as a method</li>
  <li><strong>Suspicious code</strong>a variable assigned to itself, a constant division or modulo by zero, strings compared with <code>==</code>, switch fall-through, a <code>return</code>/<code>break</code>/<code>continue</code> in <code>finally</code>, a stray <code>;</code></li>
</ul>

<h3>Details worth knowing</h3>
<dl class="meta-grid">
  <dt>Static imports</dt>
  <dd>A member brought in with <code>import static</code> and used unqualified — <code>PI</code>, <code>max(a, b)</code> — resolves to its type; a name no static import supplies is still caught.</dd>
  <dt>An annotation's import</dt>
  <dd><code>@SpringBootApplication</code> with no import is the same "cannot find symbol" — the easiest to leave behind, since the code around it still reads correctly.</dd>
  <dt>A nested type through its outer</dt>
  <dd><code>Cfg.MyProva</code> where <code>Cfg</code> declares no <code>MyProva</code> is caught, judged only when the qualifier is a type <em>this project declares</em> — so <code>com.acme.Foo</code> and a library's <code>Map.Entry</code> are left as written. <kbd>Alt</kbd> + <kbd>Enter</kbd> offers the import.</dd>
  <dt>Types</dt>
  <dd>Reference types are compared only between concrete classes, never second-guessing interface or generic code; boxing and widening are allowed. A <strong>fluent chain</strong> is checked — <code>Optional.ofNullable(repo.kind()).orElse(null)</code> returned as an <code>Integer</code> is caught — unless it is handed a lambda or method reference, whose type Bennu does not infer.</dd>
</dl>
<Callout variant="tip" title="Silent rather than wrong">
  The checks that lean on the standard library and the dependencies — unknown members, argument counts, unresolved types, compatibility, inheritance, lambda
  targets — run once a JDK is available and stay silent about anything they cannot resolve with certainty. They never report a false error.
</Callout>

<h2>While a member does not parse</h2>
<p>
  A file being typed usually has one member that does not parse yet, and the rest is checked normally. The error is charged to the <strong>member it landed
  in</strong> — method, constructor, field, initialiser — and everything outside it is validated as if the file were whole, nested classes below included.
</p>
<p>
  Inside that member you get the syntax error and nothing else: recovery reads what it can as code, so a half-closed string literal would turn its own contents
  into a page of undefined symbols. When no member can be blamed — an unbalanced brace at class level, changing what every member below is nested in — the
  whole file falls back to its syntax error alone.
</p>

<h2>Data flow</h2>
<p>Three checks follow a <em>value</em> through a method rather than reading a declaration:</p>
<ul>
  <li>a member reached on a local that is definitely <code>null</code>;</li>
  <li>a null check whose answer is already known;</li>
  <li>a value assigned to a local and overwritten before anything reads it.</li>
</ul>
<Callout variant="info" title="Deliberately narrow">
  It reads a method's statements in order and <strong>forgets everything at the first branch</strong> — an <code>if</code>, a loop, a <code>try</code>, a
  <code>switch</code> — and tracks <strong>locals only</strong>, since another method could change a field between two lines. It misses more than it finds, and
  that is the trade: a wrong flow analysis accuses working code of throwing, with no way for the reader to see why.
</Callout>

<h2>Turning a check down</h2>
<p>
  Under Project Configuration → <strong>Inspections</strong> every check has a severity — <strong>error</strong>, <strong>warning</strong>,
  <strong>weak</strong> or <strong>off</strong>. That is a policy over <em>kinds</em>: "this project does not care about unused imports". For one place, the
  source says so:
</p>
<pre><code>{@html highlightCode(`@SuppressWarnings("unused-import")
class LegacyImporter { … }`, 'java')}</code></pre>
<pre><code>{@html highlightCode(`import com.acme.Old;   // bennu:ignore unused-import`, 'java')}</code></pre>
<p>
  The annotation covers the declaration it sits on; the comment covers its own line, or the line below it. A comment naming no code silences
  everything on the line it covers. Javac's vocabulary — <code>unused</code>,
  <code>fallthrough</code>, <code>all</code> — is honoured where it overlaps, so a legacy file already carrying it need not say it twice.
</p>

<h2>Naming conventions</h2>
<p>
  A project can declare how its declarations are spelled and have every name that breaks it flagged. It is <strong>off until you turn it on</strong>, per kind of
  declaration, under <strong>Project Configuration → Naming conventions</strong> — nothing is assumed about a project that never asked.
</p>
<div class="feature-grid two-col">
  <div class="feature-card">
    <div class="fc-eyebrow">Bennu's own parser</div>
    <div class="fc-title">Java</div>
    <div class="fc-desc">Every declaration is seen, locals and parameters included.</div>
  </div>
  <div class="feature-card">
    <div class="fc-eyebrow">The language server's outline</div>
    <div class="fc-title">TypeScript, JavaScript, Rust</div>
    <div class="fc-desc">Types and their members only — locals and parameters are not in an outline, so those rows are greyed out. The server must be installed.</div>
  </div>
</div>
<p>
  Pick a convention for each kind — types, methods, fields, constants, parameters, locals, type parameters, enum constants, package segments — or leave it at
  <code>any</code>, which checks nothing. <em>Use the standard convention</em> fills in what the community uses — for Java <code>PascalCase</code> types,
  <code>camelCase</code> members, <code>UPPER_SNAKE_CASE</code> constants. The conventions are a fixed list, not patterns you write, and that is what makes the
  fix possible: a pattern can refuse a name, a convention can <em>build</em> the right one. Code templates build names with the same conventions — see
  <strong>Template reference</strong>.
</p>
<p>
  A violation is a <strong>weak warning</strong> — its own level, drawn faintly and grouped apart in Problems. A name breaking a house style is true, but it is not
  a defect.
</p>

<h3>Fixing names</h3>
<p>
  <kbd>Alt</kbd> + <kbd>Enter</kbd> on a name offers <em>Rename to <code>theRightName</code></em>. A Java local or parameter is renamed straight away, since nothing
  outside its file can refer to it. Anything a caller could use — a method, a field, a type, and everything in a server-read language — opens the rename preview
  with the name filled in. A rename does not rewrite JSP, OGNL or reflection strings, which is exactly why those fixes ask first.
</p>
<p>For more than a couple, <em>Fix naming in file</em> and <em>Fix naming in project</em> in the command palette:</p>
<ol class="step-list">
  <li>The review opens at once and fills in as the plan is built, with progress — and a <strong>Stop</strong> that still hands you what it had.</li>
  <li>Argue with it: <strong>group</strong> by file, by kind or not at all; switch a <strong>kind</strong> off wholesale, untick a <strong>group</strong> or a single
    name; <strong>filter</strong> by name. The footer always counts what Apply will do — nothing out of sight is still applied — and the list is windowed, so thousands
    of names scroll like a handful.</li>
  <li>Apply. Nothing was written before, and the whole fix is a single Undo after.</li>
</ol>
<Callout variant="warning" title="What a bulk fix refuses">
  Two names in a file becoming the same name; a spelling already used there — how a bulk fix turns compiling code into two members with one signature; a method
  overriding something from a dependency, whose name a jar fixes; and a file whose bytes are not valid in the project's declared encoding, where the editor and the
  index read it differently. Every refusal is listed with its reason.
</Callout>
<p>
  Never reported: <strong>generated code</strong> — build output, <code>@Generated</code>, a "do not edit" banner — constructors (their type is reported instead),
  <code>@Override</code> methods (the supertype's name), and platform names like <code>serialVersionUID</code>. Add your own path globs under <em>Never check</em>.
</p>
<p>
  A project rarely has one convention, so a subtree can have its own. Under <em>Exceptions</em>, name one, give it path globs, and set the conventions that apply
  inside — <strong>only</strong> those are replaced. Test sources are the usual case: <code>test00_invalid_ragioneSociale</code> mixes camelCase and snake_case on
  purpose, and an exception setting <em>method</em> to <code>any</code> under <code>**/src/test/**</code> stops reporting it while keeping the type and constant rules
  that <em>Never check</em> would drop. When two exceptions claim a file, the later one wins.
</p>

<h2>Validating the whole project</h2>
<p>
  The checks normally run on the file you are editing. To run them over <strong>every</strong> <code>.java</code> file at once, in a Maven project:
</p>
<ol class="step-list">
  <li>Open the chevron of the <strong>Build</strong> split button.</li>
  <li>Pick <em>Validate (no compile)</em> — or make it the default, so <kbd>Ctrl</kbd> + <kbd>F9</kbd> runs it.</li>
  <li>The Build window reports timing — total, average per file, the slowest file with a fast, normal or slow verdict — and every problem lands in
    <strong>Problems</strong>, grouped by file.</li>
</ol>
<p>
  <strong>Errors decide the verdict; warnings never do.</strong> A run with warnings only reads as <em>passed</em>, the counts coloured for what they are — red
  errors, yellow warnings, grey when there are none. A build and a validation cannot run at the same time.
</p>
<dl class="meta-grid">
  <dt>Cached</dt>
  <dd>Each file's result is kept against the exact project types it depends on, so an unchanged project validates instantly again, and after an edit only the changed file — and whatever its types touched — is checked.</dd>
  <dt>Warmed up</dt>
  <dd>The cache fills in the background right after indexing, so the first validation is already instant. <strong>Settings → Java → Validate project on open</strong> turns that off.</dd>
  <dt>A background citizen</dt>
  <dd>At most about half the cores by default, so the editor stays responsive. <strong>Settings → Java → Validation CPU threads</strong> caps it (1 for single-threaded); its sibling <strong>Indexing CPU threads</strong> caps the index build and the reference walk, serial by default. <strong>Cancel</strong> on the "Validating…" status stops a sweep.</dd>
</dl>

<h2>The Problems panel</h2>
<p>
  A tree grouped <strong>by severity</strong> — <strong>Errors</strong> and <strong>Warnings</strong> at the top, then <strong>Weak warnings</strong> such as naming
  and the informational levels — each split by source: a JDK node, an Encoding node, one node per file. A file with errors and warnings appears under both, with just
  that severity's rows, and every node collapses.
</p>
<ul>
  <li>It follows <strong>the file you are editing</strong> live: a fixed problem disappears, a new one appears, and that file's entry stays right after you switch away.</li>
  <li>Once <em>Validate (no compile)</em> has run, <strong>saving</strong> refreshes the whole panel quietly — so a fix that resolves an error in a <em>different</em>
    file clears there too.</li>
</ul>
<Callout variant="info" title="It is not a Java panel">
  Whatever a <strong>language server</strong> reports lands here too — rust-analyzer's <code>cargo check</code>, TypeScript's, Svelte's, Angular's on a template —
  including for files you have never opened, arriving as each check finishes, with nothing to run or arm.
</Callout>
<p>
  <strong>Only this project's files.</strong> A server reports on the whole crate graph it built, which for a <code>path</code> dependency means files of another
  repository; those are real and not yours to fix from here, so they are left out — open that project to see them. The same rule drops a registry checkout and, when
  a session sits on an outer workspace, that workspace's other members.
</p>
<p>
  The sources are kept apart on purpose: a polyglot repository can have a Java half just validated and a Rust half being checked, and either replacing the other would
  make a <code>cargo check</code> quietly erase a validation depending on which finished last.
</p>

<h3>Machine-generated expressions</h3>
<p>
  Everything that reasons about <em>types</em> — hover, the checks comparing types, completion after a dot — walks the expression it looks at, and nesting is that
  walk's cost. A generated concatenation of thousands of pieces, an unrolled query builder or a generated messages class nests one level per piece.
</p>
<Callout variant="info" title="Past about 128 levels">
  Bennu stops descending and answers <em>unknown</em> for that expression: its hover says nothing and the type checks skip it — <strong>only there</strong>. Syntax
  checks, the outline, find usages and go-to are unaffected, and hand-written code never reaches the limit: a long fluent chain is tens of levels, not hundreds.
</Callout>
