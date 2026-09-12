<script lang="ts">
  /**
   * Refactoring and intentions: the Alt+Enter list, extract and inline, moving members and classes, the create fixes and safe
   * delete, quick fixes, rename, Generate, implement/override, and spelling. Everything goes through the reference index.
   */
  import Callout from '$lib/components/shared/ui/Callout.svelte';
  import { highlightCode } from '$lib/utils/highlight';
</script>

<span class="eyebrow">Editor</span>
<h1>Refactoring &amp; intentions</h1>

<p class="doc-lead">
  The edits Bennu makes for you. All of them go through the reference index navigation uses, which is what separates a rename from a
  search-and-replace: the index knows which <code>getName</code> is <em>this</em> one.
</p>

<h2>Where they are</h2>
<dl class="meta-grid">
  <dt>The right-click menu</dt>
  <dd>Cut · Copy · Paste, and the semantic actions — <strong>Go to declaration</strong>, <strong>Find usages</strong>, <strong>Rename</strong>, <strong>Generate</strong>, <strong>Save</strong> — on the symbol <strong>under the pointer</strong>: right-clicking moves the caret there first.</dd>
  <dt><kbd>Alt</kbd> + <kbd>Enter</kbd></dt>
  <dd>The <strong>intentions</strong> popup at the caret — everything available there, <kbd>↑</kbd>/<kbd>↓</kbd> to move, <kbd>Enter</kbd> to apply, <kbd>Esc</kbd> to dismiss. The entry to the generators, the refactorings and the quick fixes.</dd>
</dl>

<h2>Intentions</h2>
<p>
  On a <strong>type that is not imported</strong>, <strong>Import '…'</strong> adds the <code>import</code> — after the package, sorted among the
  others. When several classes share the name each is its own entry, so you pick the package; a type in the same package, in
  <code>java.lang</code>, or covered by a wildcard is not offered, needing none.
</p>
<table>
  <thead><tr><th>Offered on</th><th>Becomes</th></tr></thead>
  <tbody>
    <tr><td><code>logger.info("user " + id + " logged in")</code></td><td><code>logger.info("user &lbrace;&rbrace; logged in", id)</code> — parameterized logging; a trailing exception stays last</td></tr>
    <tr><td><code>x.equals("literal")</code></td><td><code>"literal".equals(x)</code> — never throws when <code>x</code> is null</td></tr>
    <tr><td><code>list.size() == 0</code></td><td><code>list.isEmpty()</code></td></tr>
    <tr><td><code>flag == true</code></td><td><code>flag</code></td></tr>
    <tr><td><code>!(a == b)</code></td><td><code>a != b</code></td></tr>
  </tbody>
</table>
<Callout variant="info" title="A file and its type disagree">
  <code>Foo.java</code> holding <code>public class Bar</code> does not compile, and it happens both ways — a class renamed in a text editor, or a
  file copied and the class inside renamed. So the popup offers <strong>both ways out</strong>: rename the type to match the file, or the file to
  match the type. Picking one for you would be wrong half the time, in the direction that loses the name you meant to keep. Renaming the type goes
  through the rename preview, so every use follows.
</Callout>

<h2>Extract and inline</h2>
<p>
  One <kbd>Alt</kbd> + <kbd>Enter</kbd> list, offered from what is selected: a run of statements means <em>extract method</em>, a caret in an expression
  <em>extract variable</em>, a caret on a name one of the inlines, a caret on a member's header one of the moves. Every one arrives as a single undo.
</p>

<h3>Extract method</h3>
<p>
  The selected statements become a method and a call. The locals it reads become parameters, typed as declared; a local it produces that
  <em>outlives</em> the selection, and that the code after reads, becomes the return value — a name declared inside a loop or a branch of the selection
  dies with it, whatever the later code calls its own variables.
</p>
<ul>
  <li>A <code>static</code> method extracts a <code>static</code> one; a generic one carries the type parameters its signature needs, with their bounds —
    the <code>throws</code> clause counts.</li>
  <li>The checked exceptions the moved body can raise are declared on it, and the body is re-indented rather than pasted.</li>
  <li>A name caught as <code>A | B</code> has no single type to write in a signature, so a selection reading one is refused.</li>
</ul>

<h3>Extract variable · extract constant</h3>
<p>
  The expression gets a name: a local above its statement, or a <code>private static final</code> beside the class's fields when it is constant to read.
  The type is resolved, so the declaration says <code>List&lt;String&gt;</code> rather than <code>var</code>, and its import comes along.
</p>
<ul>
  <li>The name steps aside from anything in scope — a field the method reads keeps its name, and the new local becomes <code>value2</code> rather than
    quietly taking over every later mention.</li>
  <li>A constant is refused in the type's own header, where <code>@SuppressWarnings</code> sits before the <code>&#123;</code> and a field would not be in scope.</li>
  <li>Where the surrounding code decides the type — an argument, a <code>return</code>, an arm of a conditional — a type it works out but cannot write is a
    refusal: <code>var</code> there would have nothing to infer from.</li>
  <li>A <strong>captured wildcard</strong> — <code>a.annotationType()</code> gives <code>Class&lt;? extends Annotation&gt;</code> — is refused at a field and
    keeps <code>var</code> at a local: a capture has no name anyone can type. An expression it cannot type at all gets <code>var</code>, as javac would infer.</li>
</ul>

<h3>Invert if · merge nested if</h3>
<pre><code>{@html highlightCode(`if (a) X else Y      →      if (!a) Y else X`, 'java')}</code></pre>
<p>
  The condition is negated the way a person would: a comparison flips its operator, a <code>!</code> comes off rather than doubling, and
  <code>&amp;&amp;</code> becomes <code>||</code> over negated halves. A shape with no exact opposite is wrapped, not guessed at. Two <code>if</code>s with
  nothing between them join into one — refused when either has an <code>else</code>, which would run in a case the merged test no longer tells apart.
</p>

<h3>Split · join · <code>var</code></h3>
<p>
  A declaration separates from its assignment and joins back, and a written type swaps with <code>var</code> both ways — going back asks the project what
  the initialiser's type is, and declines rather than leaving <code>var</code>. Splitting a <code>final</code> local is refused: Java allows that shape only
  where it can prove the variable unset. The four things <code>var</code> cannot read — a lambda, a method reference, a bare <code>&#123;…&#125;</code>,
  <code>null</code> — each say so by name.
</p>

<h3>Introduce field</h3>
<p>A local becomes a field of its class, and its <strong>initialisation stays where it ran</strong>:</p>
<pre><code>{@html highlightCode(`int total = a + b;`, 'java')}</code></pre>
<pre><code>{@html highlightCode(`private int total;
…
total = a + b;`, 'java')}</code></pre>
<p>
  Initialising the field at its declaration would run the expression at construction time — a different program whenever it reads a parameter, throws,
  or costs anything. The field joins the other fields, except for a local in an <strong>initialiser block</strong>, where it goes above the block, since a
  field may only be read by initialisers declared after it.
</p>
<ul>
  <li>A local of a <code>static</code> method makes a <code>static</code> field; a <code>var</code> local still gets a written type.</li>
  <li>Refused: a name the class already declares; a local typed with its method's own type parameter; an interface, whose fields must be initialised at
    their declaration; a record, which may not have instance fields; a local inside an anonymous class, whose field would land on the class around it.</li>
</ul>

<h3>Replace <code>if</code> chain with <code>switch</code></h3>
<p>
  An <code>if</code> / <code>else if</code> ladder testing one value against constants becomes a <code>switch</code>, with the <code>break</code>s the compiler
  accepts — none after an arm that already returns or throws, where it would be unreachable. Enum constants lose their type, as a <code>switch</code> writes them.
</p>
<Callout variant="warning" title="When it is left alone">
  The subject must be something re-reading cannot change, so a chain testing <code>kind()</code> stays — it ran the call once per rung. So does
  <code>"a".equals(s)</code>, null-safe where <code>switch (s)</code> throws. An arm ending in a <code>try</code> or a nested <code>switch</code>, where a
  <code>break</code>'s reachability cannot be read off the text, refuses the whole conversion. And a <code>switch</code> over a <code>String</code> is
  <strong>Java 7</strong>: below that, it is said rather than written.
</Callout>

<h3>Pull up · push down · move member</h3>
<p>
  A member changes the type it belongs to; the three differ only in which type, and the menu is where you choose. Every target written in this file — each
  <code>extends</code> and <code>implements</code>, each subtype declared here — is its own row, so the common move is one keystroke. The rows ending in
  <strong>…</strong> open a filterable list of every candidate the <em>project index</em> knows: a superclass in another file, a subtype elsewhere, any type
  for a sideways move. A target in another file is edited too, with the imports the member reads carried over.
</p>
<p>
  First it checks <strong>who still needs the member where it is</strong>, across the whole project: one something calls, or a subclass overrides, does not go
  down or sideways. Then what it takes from the type it leaves — and each refusal names what keeps the member there:
</p>
<ul>
  <li>a field it reads that the target does not have — a method reading <code>count</code> will not go where there is no <code>count</code>;</li>
  <li>the class's type parameter, where the target never declared it;</li>
  <li>its own class by name — a factory returning it, a <code>new</code> of it — which would mean the same class wherever it lands;</li>
  <li>a <code>super</code> call, meaning another method once moved; an <code>@Override</code>, a promise about the type it is in; no body, a contract rather than code;</li>
  <li>a name the target already declares;</li>
  <li>a call to one of its own <strong>overloads</strong>, which reads as recursion and is not;</li>
  <li>a pull up into a generic supertype whose type arguments this class fixes, where the member would meet a type variable in place of its concrete type.</li>
</ul>
<dl class="meta-grid">
  <dt>Another package</dt>
  <dd>A type the member reached through its own package resolves to nothing there and has no import to carry, so only what is self-contained moves, widened to <code>protected</code> if it was package-private.</dd>
  <dt>Sideways</dt>
  <dd>For <code>static</code> members needing nothing from where they were: an instance member would find <code>this</code> pointing elsewhere.</dd>
  <dt>Into an interface</dt>
  <dd>A method becomes <code>default</code> and drops the modifiers it may not carry; a <code>private static</code> helper does not stay private. <code>default</code> and <code>static</code> interface methods are <strong>Java 8</strong> — below it, you are told. A <code>static</code> method called from anywhere is refused: static interface methods are not inherited (JLS §8.4.8).</dd>
  <dt>Back into a class</dt>
  <dd>It loses that <code>default</code> and keeps the <code>public</code> the interface gave it; an <code>enum</code> whose constants had no <code>;</code> gets one.</dd>
  <dt>A <code>private</code> member pulled up</dt>
  <dd>Widened to <code>protected</code> where the class it leaves still reads it — the row says so. One nothing reads goes up as written.</dd>
</dl>
<Callout variant="info" title="serialVersionUID stays put">
  It and its serialization neighbours are read <em>by name</em> at run time, so no source mentioning them proves anything.
</Callout>

<h3>Move class</h3>
<p>
  A type written inside another gets its own file beside it — the same folder, which is "the same package", so every unqualified mention in the package still
  resolves. It takes the package line and the imports it reads, and drops the modifiers a top-level type may not carry: <code>static</code>,
  <code>private</code>, <code>protected</code>. It stays nested when:
</p>
<ul>
  <li>it is <strong>inner</strong> — a nested <code>class</code> without <code>static</code> holds a reference to its outer instance, which a top-level type has nowhere to keep;</li>
  <li>it reads the outer class's own members, or names a <em>sibling</em> nested type, which would resolve to nothing from outside;</li>
  <li>this file spells it <code>Outer.Inner</code> anywhere, or any <em>other</em> file mentions it at all;</li>
  <li>the outer class reads its <code>private</code> members — a private constructor included.</li>
</ul>

<h3>Inline variable · inline method</h3>
<p>
  The value goes back where the name was, parenthesised wherever the surrounding expression binds tighter. Which occurrences count is a question of scope:
  the ones in the declaration's own scope, after it, that read the variable — not <code>Math.max</code>, not a selector after a dot, not a lambda parameter,
  not a same-named variable in the next block.
</p>
<p>
  It declines where the declaration did work the expression alone cannot: <code>final byte n = 5;</code> narrows a constant only an assignment may, and a value
  moved into a lambda becomes a capture that may only read unchanging locals. A one-expression method goes back into its call with its arguments substituted
  structurally, so a local sharing a parameter's name is untouched and a non-simple argument is parenthesised.
</p>

<h2>Creating what is missing, and deleting safely</h2>
<div class="feature-grid two-col">
  <div class="feature-card">
    <div class="fc-eyebrow">On a call to a method that does not exist</div>
    <div class="fc-title">Create method</div>
    <div class="fc-desc">Written just below the calling method. The call site is the specification: argument types and names become the signature, how the result is used the return type — nothing means <code>void</code>, a condition <code>boolean</code> — and a <code>static</code> caller gets a <code>static</code> method. The body throws, so it compiles and fails loudly rather than returning a plausible <code>null</code>.</div>
  </div>
  <div class="feature-card">
    <div class="fc-eyebrow">On <code>order.total(label)</code></div>
    <div class="fc-title">Create method in the receiver's class</div>
    <div class="fc-desc">Written at the end of <strong>that class's file</strong>, <code>public</code>, with the imports its types need, and the file opens. Declined for a receiver from a <strong>jar</strong>, a class that <strong>already declares</strong> the name at any arity, or a receiver whose type does not resolve.</div>
  </div>
  <div class="feature-card">
    <div class="fc-eyebrow">On a type that does not resolve</div>
    <div class="fc-title">Create class</div>
    <div class="fc-desc">The file is created beside the one naming it and opened. The use decides the kind: an <code>implements</code> clause makes an <code>interface</code>, after <code>@</code> an annotation, anything else a class. A name that already has a file is refused.</div>
  </div>
  <div class="feature-card">
    <div class="fc-eyebrow">On a member</div>
    <div class="fc-title">Safe delete</div>
    <div class="fc-desc">Removes it and its doc comment — or refuses and lists every use with file, line and text, because "it is used" is not an answer. Declined outright on an override of something from a jar, a method declared at several levels of one hierarchy, one something implements, and anything annotated, which a framework may reach by name.</div>
  </div>
</div>
<Callout variant="tip" title="A refactoring that cannot be done says why">
  It stays in the list, greyed, with the reason: <em>"the selection produces <code>total</code> and <code>count</code>, and a method can only return
  one"</em> tells you what to change. So do a <code>return</code> leaving the selection, a variable assigned again later, a value with a side effect read
  twice, an overloaded call its arguments cannot resolve, a <code>var</code> local whose type cannot go in a signature.
</Callout>
<p>
  On a file a <strong>language server</strong> serves — Rust, TypeScript, Python — the same list carries the server's own refactorings, with its full type
  knowledge and its disabled ones with their reasons. The gesture is the same.
</p>

<h2>Quick fixes</h2>
<p>With the caret on a <strong>diagnostic</strong>, <kbd>Alt</kbd> + <kbd>Enter</kbd> offers the repair, not only the sentence:</p>
<table>
  <thead><tr><th>Diagnostic</th><th>Fix</th></tr></thead>
  <tbody>
    <tr><td>Unused, duplicate or redundant import</td><td>Remove it, with its line</td></tr>
    <tr><td>Unhandled checked exception</td><td>Add <code>throws</code> — extending a clause already there — or surround the statement with <code>try</code>/<code>catch</code></td></tr>
    <tr><td>Non-exhaustive enum switch</td><td>Write the missing cases, in the form the switch uses — arrows or colons, never a mix</td></tr>
    <tr><td>Strings compared with <code>==</code></td><td><code>equals</code>, the literal on the receiver side; <code>!=</code> keeps its negation</td></tr>
    <tr><td>Switch fall-through</td><td>Add the <code>break;</code>, indented with its group</td></tr>
    <tr><td>A stray <code>;</code></td><td>Remove it</td></tr>
    <tr><td>A missing import</td><td>Add it, one entry per candidate package</td></tr>
  </tbody>
</table>
<p>
  A fix is keyed to the <em>kind</em> of diagnostic and reads the source itself, never guessing from the wording. The two that need types are recomputed from
  the analysis that raised the diagnostic, so a fix that appears is one that clears the squiggle.
</p>

<h2>Rename</h2>
<ol class="step-list">
  <li>Put the caret on a symbol and press <kbd>Shift</kbd> + <kbd>F6</kbd>. A small field opens at the caret.</li>
  <li><kbd>Enter</kbd> applies — or <kbd>Shift</kbd> + <kbd>Enter</kbd> first opens a <strong>preview</strong> of every edit, grouped by file.</li>
  <li>Either way it goes through the editor, so one <kbd>Ctrl</kbd> + <kbd>Z</kbd> undoes it all — and either way the file moves when the rename requires it.</li>
</ol>
<table>
  <thead><tr><th>The caret on</th><th>What is rewritten</th></tr></thead>
  <tbody>
    <tr><td>A <strong>local</strong> or <strong>parameter</strong></td><td>Scope-exact: that method only — never a same-named variable elsewhere, or a field.</td></tr>
    <tr><td>A <strong>method</strong> or <strong>field</strong></td><td>Its declaration and every use in the project, including through a library generic — a lambda parameter off <code>list.stream().map(…)</code>. A method carries its whole <strong>override family</strong> and all its <strong>overloads</strong>: to a caller they are one.</td></tr>
    <tr><td>A <strong>record component</strong>, or a field <strong>Lombok</strong> writes accessors for</td><td>The field and the calls to accessors nobody wrote — <code>failure.sourcePath()</code>, <code>order.getCustomerName()</code> — getters, setters and <code>@With</code> copies. A hand-written accessor is its own declaration and stays.</td></tr>
    <tr><td>A <strong>class</strong> or <strong>interface</strong></td><td>Declaration, references, <code>import</code>s and Spring <code>&lt;bean class="…"&gt;</code> entries — a Struts <code>&lt;action class="…"&gt;</code> names a bean id and is left. A public top-level type's <strong>file is renamed with it</strong>; a nested type's file is its outer type's.</td></tr>
  </tbody>
</table>
<p>
  A member reached through <code>import static</code> carries the import and its bare calls, and a <strong>method reference</strong> —
  <code>Failure::sourcePath</code> — moves with its method. Edits that cannot be pinned exactly, like an overloaded method's call sites, are marked for review
  in the preview rather than applied silently. It answers once the index is warm; OGNL and JSP references are not rewritten yet.
</p>
<Callout variant="warning" title="Refused when it would break an override that cannot follow">
  A method implementing an interface from a dependency has its name fixed by that dependency; renaming only your side leaves a class that no longer implements
  what it declares. The preview still shows what it would have done and names the library type, but will not apply it.
</Callout>

<h2>Generate</h2>
<p>
  <kbd>Alt</kbd> + <kbd>Insert</kbd> builds a constructor, getters, setters or both from the class's fields: pick a mode, tick the fields, choose fluent or
  plain setters and camelCase or snake_case accessors, watch the live preview, and <kbd>Ctrl</kbd> + <kbd>Enter</kbd> inserts it at the caret.
  <em>From a template…</em> there writes with a code template of yours instead — see <strong>Code templates</strong>.
</p>

<h2>Implement / override methods</h2>
<p>
  With the caret in a class, <kbd>Alt</kbd> + <kbd>Enter</kbd> → <strong>Implement / override methods…</strong> — or the command palette — lists what the class
  inherits and may override, <strong>grouped by the declaring type</strong>. Tick them — a group's box takes all — and <kbd>Ctrl</kbd> + <kbd>Enter</kbd> writes
  them inside the closing brace, as one undo step.
</p>
<ul>
  <li><strong>Abstract methods start ticked</strong>, and nothing else: those the compiler demands. Their body throws <code>UnsupportedOperationException</code> —
    a stub returning <code>null</code> would compile, run and lie. A concrete one's body starts with <code>super.…</code>.</li>
  <li>Only what Java lets you override: never a <code>static</code>, <code>final</code> or <code>private</code> method, a constructor, a package-private method
    from another package, or one this class declares — matched on parameter types, so unwritten overloads remain.</li>
  <li>The types they mention are <strong>imported in the same step</strong>: generated code that does not compile is not generated code.</li>
</ul>

<h2>Spelling</h2>
<p>
  Opt-in per project, in Project Configuration → <strong>Spelling</strong>. After the English and Italian dictionaries download, Bennu checks your
  <strong>declared names</strong> — split by camelCase, snake_case and kebab-case — and your <strong>comments</strong>. A misspelled word is a hint;
  <kbd>Alt</kbd> + <kbd>Enter</kbd> replaces it with a suggestion or <strong>adds it to a project or global dictionary</strong>. Common programming
  abbreviations are allowed, so it stays quiet on jargon.
</p>
