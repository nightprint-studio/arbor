<script lang="ts">
  /**
   * Completion: asking for it, the order candidates come in, how a name is matched, what accepting writes, type names and
   * imports, annotations, abbreviations and postfix templates, members that do not exist yet, and generated members.
   */
  import Callout from '$lib/components/shared/ui/Callout.svelte';
  import { highlightCode } from '$lib/utils/highlight';
</script>

<span class="eyebrow">Editor</span>
<h1>Completion</h1>

<p class="doc-lead">
  What the popup knows, where each suggestion comes from, and the members that exist without being written anywhere.
</p>

<h2>Asking for it</h2>
<p>
  Typing <code>.</code> after an expression offers the members. <kbd>Ctrl</kbd> + <kbd>Space</kbd> — or <kbd>Ctrl</kbd> + <kbd>Shift</kbd> +
  <kbd>Space</kbd> — asks explicitly, anywhere. In Java the answers come from the project index and appear once it is warm; edits are indexed
  again in the background as you type, so completion and go-to follow your changes without reopening anything.
</p>
<Callout variant="warning" title="On macOS the chord is Cmd + Shift + Space">
  Not a style choice. macOS claims the whole <kbd>Ctrl</kbd> + <kbd>Space</kbd> family for switching input source, <em>above</em>
  applications: those chords produce no key event in any program. <kbd>Cmd</kbd> + <kbd>Space</kbd> is Spotlight, which is what the
  <kbd>Shift</kbd> is for.
</Callout>
<ul>
  <li>When an explicit request finds nothing, the footer says so for a moment — <em>No suggestions here</em>, or <em>No engine answered for
    this file</em> when no server or index is up. Silence would look like a shortcut that never arrived, and those two have opposite fixes.</li>
  <li>If a shortcut seems to do nothing at all, turn on <strong>Show keyboard inputs</strong> (<kbd>Alt</kbd> + <kbd>Shift</kbd> +
    <kbd>K</kbd>) and press it again: the overlay draws every chord the window receives. Nothing drawn means the key never arrived — a
    conflict outside Bennu.</li>
  <li><strong>Command languages ask on every position.</strong> A <code>.dev</code> scenario or a <code>.dig</code> line is words and
    arguments rather than dotted paths, so with the caret after <code>unlock&nbsp;</code> the server — <strong>nd-dig-lsp</strong>, geode's
    own — is asked too, not only after a trigger character.</li>
</ul>

<h2>The order things are offered in</h2>
<p>
  What comes first is worked out from what the engine already knows, and it decides most of what completion feels like. Strongest first:
</p>
<ol class="step-list">
  <li><strong>What the position wants.</strong> The only term about the <em>hole</em> rather than the candidate. In
    <code>String name = order.</code> there are forty members on <code>order</code> and a handful that can be written there — and nothing
    about <code>order</code> says which, because the constraint is left of the <code>=</code>. It is read from a declaration with a written
    type, an assignment to something typed, a <code>return</code>, and a condition, which wants a <code>boolean</code> and turns a member
    list into its predicates. A <code>void</code> method sinks where a value is wanted. It <em>ranks</em>, never filters: the match is by
    name, so a real subtype is a miss, and hiding misses would hide the right answer.</li>
  <li><strong>What you picked last time.</strong> A fact about you, not the code: on a <code>List</code> you reach for <code>stream</code>,
    not <code>listIterator</code>. Kept per <em>declaring type</em>, so what is learned about <code>java.util.List</code> carries to every
    receiver inheriting it. Frequency and recency both count, so a fresh choice can overtake an old habit and one stray pick cannot bury a
    name you use constantly. It lives for the session and is never written to disk — a persisted ranking that drifted would be invisible.</li>
  <li><strong>What the receiver is.</strong> After a <em>type</em> — <code>Color.</code> — the statics are the answer and a constant goes to
    the very top, instance members to the bottom. Through a <em>value</em> — <code>color.</code> — the other way round, more gently: a
    static through an instance compiles, it is just rarely meant.</li>
  <li><strong>How far up the hierarchy it was found.</strong> What the receiver's own class declares beats what it inherited — the further up, the weaker.</li>
  <li><strong><code>java.lang.Object</code>.</strong> Its members match every prefix and are hardly ever wanted, so <code>list.</code> opens
    on <code>add</code>, not on <code>clone</code> and <code>equals</code>.</li>
  <li><strong>Deprecated.</strong> Still offered, since you may be reading old code, but last — on your own source, since library members
    carry no annotations in compiled form.</li>
  <li><strong>What this file already uses.</strong> A name already in the buffer is likely wanted again. It decides between otherwise-equal
    candidates, and cannot do more.</li>
</ol>
<p>
  Keywords come below anything the index resolved. Words <em>scraped from the buffer</em> are offered only when the index answered nothing —
  before it finished building, or in a file no project owns: they are a regular expression over the text, and beside resolved names they bury
  them under look-alikes. How well what you typed matches still decides between neighbours.
</p>

<h3>Type names</h3>
<p>A simple name usually resolves to several types, so type names get two terms of their own:</p>
<dl class="meta-grid">
  <dt>What this file has said</dt>
  <dd>Nothing outranks it: a type the file imports, then a package it wildcard-imports, then its own package, which needs no import.</dd>
  <dt>What the project imports</dt>
  <dd>Every <code>import</code> in the codebase is somebody choosing a candidate, and the aggregate predicts the next choice. It is weighed against <strong>distance</strong> — the nearest package by shared prefix, so a sibling beats a cousin — with the JDK ahead of the rest of the classpath, because a name matching both a JDK type and something in a jar you never opened is almost always the JDK one.</dd>
</dl>
<p>
  The two are weighed, not ordered, and the balance is the point: <code>java.util.List</code> imported in four hundred files beats an
  <code>it.acme.model.List</code> nobody imports, though the second is nearer. A type imported <em>once</em> beats nothing — that is a
  coincidence, not evidence.
</p>
<Callout variant="tip" title="Turning the project's imports off">
  <strong>Settings → Completion → Order type names by what this project imports</strong> switches the second term off at once. What the file
  itself imports still comes first — that was never a statistic.
</Callout>

<h3>Where the import counts come from</h3>
<p>
  They are counted while the <strong>index builds</strong>, in the pass that already parses every file — one lookup per <code>import</code>
  line. Nothing is written to disk: a saved copy would go stale, and a project that dropped a library would go on recommending it with nothing
  saying why. They are not updated as you type either — whether a codebase uses <code>java.util.List</code> does not change because you saved
  a file, and an order shifting under you would be worse than one a rebuild behind. They refresh with the index.
</p>
<p>
  A count is used as a <strong>band</strong>: each step a doubling, stopping at 128. Four imports against eight is real; four hundred against
  eight hundred is not, and a ranking that pretended otherwise would pin one ubiquitous type to the top for ever. A wildcard counts for the whole
  package, one band weaker.
</p>

<h2>How a name is matched</h2>
<p>
  <strong>A name does not have to be spelled the way it is declared.</strong> What you type is matched three ways, best first, and which one
  matched is part of the order:
</p>
<table>
  <thead><tr><th>Match</th><th>Typed</th><th>Reaches</th></tr></thead>
  <tbody>
    <tr><td>An exact prefix</td><td><code>toLo</code></td><td><code>toLowerCase</code></td></tr>
    <tr><td>A prefix ignoring case</td><td><code>TOLO</code></td><td><code>toLowerCase</code> — a shift held one letter too long</td></tr>
    <tr><td>The camel humps</td><td><code>tolc</code>, <code>aAE</code>, <code>SBA</code>, <code>MAXV</code></td><td><code>toLowerCase</code>, <code>addAllElements</code>, <code>SpringBootApplication</code>, <code>MAX_VALUE</code> — an underscore starts a word too</td></tr>
  </tbody>
</table>
<p>
  It is <strong>one rule for every kind of candidate</strong> — a member, a local, a static import, a class name.
</p>
<p>
  <strong>Settings → Completion → Popup</strong> decides when the popup arrives and how forgiving it is: whether it opens on its own while you
  type, after how long a pause, or only on the chord; and <em>case-sensitive matching</em>. That last is about case, not humps — off, only the
  first letter must agree, the one that carries information in Java since a capital means a type; on, every letter must, so <code>aAE</code>
  still reaches <code>addAllElements</code> and <code>aae</code> no longer does.
</p>

<h2>A name with nothing to its left</h2>
<p>Java says exactly what a bare identifier can mean, and the popup offers those things nearest first:</p>
<ol class="step-list">
  <li><strong>What the scope binds</strong> — locals, method and lambda parameters, the <code>for</code> variable, a try-with-resources, a
    <code>catch</code> parameter, a pattern variable from <code>o instanceof Foo f</code>. A variable of this block beats a parameter of the
    method; in one block, the line above beats twenty lines up. A shadowing name is offered once, as the inner one — what the name resolves to.</li>
  <li><strong>The enclosing type's own members</strong>, and what it inherits, without <code>this.</code>.</li>
  <li><strong>What an <code>import static</code> brought in</strong>, with the type it came from on the row — the one case where nothing else
    on screen says where a bare name is from.</li>
  <li><strong>Class names</strong>, from the type-name index.</li>
</ol>
<p>
  A local is not in scope inside its own declaration, so <code>int counted = coun</code> offers <code>counter</code>, not <code>counted</code>.
  In a <code>static</code> method an instance member is not offered at all: it would not compile.
</p>
<Callout variant="info" title="Types or names first? Java's own convention decides">
  <code>Str</code> is a type being written and <code>str</code> a variable, and no score makes one the other. A capitalised name with a
  lowercase letter leads with types; anything else leads with what is in scope, where a <code>SCREAMING_CASE</code> constant of your own class
  lives. Both lists are always offered, so a prefix read the "wrong" way costs a scroll, not an answer.
</Callout>

<h2>What accepting one writes</h2>
<p>
  A method is a call: accepting <code>size</code> writes <code>size()</code>. When it takes arguments the caret lands <strong>between the
  parentheses</strong>, where the parameter hints strip is about to describe them; when it takes none, after them.
</p>
<ul>
  <li>Parameter <em>names</em> are not written — tabbing through <code>put(key, value)</code> placeholders to delete them is slower than typing,
    and a class file carries no names anyway.</li>
  <li>When the call is <strong>already written</strong> — <code>t.si()</code> with the caret after <code>si</code>, correcting a name — the
    parentheses are left alone, not doubled.</li>
  <li>An <strong>overloaded</strong> method is <em>one row</em>, its shape saying <em>+2 overloads</em>: three rows would be three chances with
    one outcome, pushing other members off the popup. The parameter hints show the whole set once you type inside the parentheses. A method
    that merely <strong>overrides</strong> an inherited one appears once; a <code>private</code> member of another class is not offered.</li>
</ul>

<h2>Reading a row</h2>
<dl class="meta-grid">
  <dt>The icon</dt>
  <dd>What it <strong>is</strong>.</dd>
  <dt>The name</dt>
  <dd>What it is <strong>called</strong>, with the letters you typed marked. A deprecated one is struck through.</dd>
  <dt>The shape</dt>
  <dd><code>(String, int) : void</code>.</dd>
  <dt>The right edge</dt>
  <dd>Where it <strong>comes from</strong> — what tells <code>List.of</code> from <code>Set.of</code>, and an inherited method from one your class declares.</dd>
</dl>
<p>
  The <strong>documentation</strong> of the highlighted row appears beside the list — the card hover draws, from the same answer, so the two
  cannot describe one member two ways. It is fetched only for the row you highlight: a library's documentation is read from its sources archive,
  and doing it for four hundred rows would put an archive read per candidate on the keystroke that opened the popup.
</p>

<h2>Type names and their imports</h2>
<p>
  Typing a <strong>capitalised name</strong> (not after a dot) offers every class matching it across the JDK, your dependencies and your
  project, each with its package — and <em>(+N more)</em> when several packages declare the name. Within each match tier your project's types
  come before a jar's, and shorter names before longer.
</p>
<ul>
  <li>Accepting a name that maps to a <strong>single</strong> class <strong>adds its import</strong> — turn that off with Settings → Completion →
    <em>Auto-import on accept</em>. An ambiguous name inserts just the name; <kbd>Alt</kbd> + <kbd>Enter</kbd> → <strong>Import '…'</strong>
    picks the package.</li>
  <li><strong>A qualified name completes one segment at a time</strong>: <code>import org.</code> offers <code>springframework</code>, and
    <code>import org.springframework.boot.</code> offers its packages beside its classes.</li>
  <li>A receiver <strong>not imported yet</strong> still completes: <code>Arrays.</code> without the import offers its members, and accepting
    one adds the import too — for an unambiguous name only.</li>
  <li>A type receiver offers its <strong>nested types</strong> beside the statics — <code>Map.</code> offers <code>Entry</code>.</li>
</ul>
<p><strong>A nested type completed by its simple name is written through its outer</strong>, since <code>Inner</code> alone is not a name Java resolves:</p>
<pre><code>{@html highlightCode(`import it.acme.web.ConfigurazioneCors;
…
ConfigurazioneCors.MyProva prova;    // accepting "MyProva" writes this`, 'java')}</code></pre>
<p>
  One import serves every nested type of that class, the form people write by hand. Where the simple name is already in scope nothing is added:
  inside the outer class or a sibling nested in it, and in a file that imports the nested type outright.
</p>

<h3>Other languages</h3>
<p>
  In <strong>TypeScript</strong> and <strong>JavaScript</strong> — and in a <code>.svelte</code> file and an Angular project's templates — the popup
  is the language server's: <code>typescript-language-server</code>, <code>svelteserver</code>, <code>ngserver</code>. Install it from Settings →
  Language Servers; without one the file is still coloured, folded and edited, with nothing to suggest.
</p>
<Callout variant="info" title="A JSP's &lt;script&gt; is answered by Bennu">
  No language server will serve a JSP — a template that <em>prints</em> JavaScript, half of it <code>&lt;%= %&gt;</code> holes. The names the block
  declares (<code>var</code>, <code>function</code>, <code>foo: function (…)</code> and their parameters) come first, then the browser globals.
  After <code>document.</code>, <code>location.</code>, <code>Math.</code> and the rest of the platform, the members are <strong>read off the object
  itself</strong> in the engine the page runs in — not a list that could be stale. Hover says the same. What it cannot know is jQuery:
  <code>$</code> is offered, <code>$(…).</code> has no object to read.
</Callout>

<h2>Annotations</h2>
<p>
  Typing <strong><code>@</code></strong> opens the popup on its own, with <strong>annotation types only</strong> — from every type on the classpath to
  the few hundred annotations on it, so the list is short and nearly always holds the answer.
</p>
<ul>
  <li>It narrows again from where the caret is: above a field, one whose <code>@Target</code> only allows a method sorts below the ones that belong
    there. Ranked, not hidden — an annotation from unread bytes reports no target, and reading that as a refusal would hide it.</li>
  <li>The list is complete from the <strong>first letter</strong>: what is capped is how many annotations are offered, not how many names are looked
    at, so <code>@S</code> reaches <code>SuppressWarnings</code> past the hundreds of classes also starting with <code>S</code>.</li>
  <li>Accepting one adds its import, and the ones you accept come first for as long as the project stays open — never written to disk, never above an
    import the file already made.</li>
</ul>

<h2>Abbreviations</h2>
<p>
  Type <code>psf</code> and the list offers <strong>public static final</strong>, with <code>psfi</code> and <code>psfs</code> under it for
  <code>int</code> and <code>String</code>. These are IntelliJ's own Java abbreviations, deliberately — fifteen years of <code>psf</code> should work
  here without configuring anything.
</p>
<table>
  <thead><tr><th>Family</th><th>Abbreviations</th></tr></thead>
  <tbody>
    <tr><td>Modifiers</td><td><code>psf</code>, <code>psfi</code>, <code>psfs</code>, <code>prsf</code>, <code>prsfi</code>, <code>prsfs</code>, <code>psvm</code> for a <code>main</code></td></tr>
    <tr><td>Printing</td><td><code>sout</code>, <code>souf</code>, <code>serr</code></td></tr>
    <tr><td>Statements</td><td><code>fori</code>, <code>ifn</code>, <code>inn</code>, <code>thr</code></td></tr>
  </tbody>
</table>
<ul>
  <li>Several blanks are walked with <kbd>Tab</kbd>, and the body is <strong>re-indented</strong> to where it lands — a <code>psvm</code> three levels
    into a class arrives lined up.</li>
  <li>They are <em>added</em> to what the index found, never substituted: a field called <code>psfCount</code> still appears, under them.</li>
  <li>Offered only where a bare word is typed — after a <code>.</code> the list answers "what members does this have".</li>
  <li>Abbreviations of your own sit beside them, written as code templates — see <strong>Code templates</strong>.
    Yours are not Java's: the language in a template's name says where it is offered, so a <code>dbg.rs.jinja</code>
    expands in a Rust file, answered by rust-analyzer, and one written with no language expands in any file. The
    built-in family above stays Java's.</li>
</ul>
<Callout variant="info" title="What is deliberately missing">
  The abbreviations that must <em>read the surrounding code</em> to be right — IntelliJ's <code>iter</code>, inferring the collection in scope. A
  version that guesses writes a name that is not there.
</Callout>

<h2>Postfix templates</h2>
<p>Write the expression first, then say what to do with it:</p>
<pre><code>{@html highlightCode(`order.getTotal().nn`, 'java')}</code></pre>
<pre><code>{@html highlightCode(`if (order.getTotal() != null) {
    |
}`, 'java')}</code></pre>
<p>
  The point is not keystrokes: it lets you think in the order you think — the value, then the control flow around it — instead of committing to an
  <code>if (</code> before you know what goes in it. They appear in the completion list under the name you type, with IntelliJ's names on purpose.
</p>
<table>
  <thead><tr><th>Family</th><th>Templates</th></tr></thead>
  <tbody>
    <tr><td>Null checks</td><td><code>.nn</code>, <code>.null</code></td></tr>
    <tr><td>Control flow</td><td><code>.if</code>, <code>.else</code>, <code>.while</code>, <code>.for</code>, <code>.fori</code>, <code>.forr</code>, <code>.switch</code>, <code>.try</code>, <code>.synchronized</code></td></tr>
    <tr><td>Statements</td><td><code>.var</code>, <code>.return</code>, <code>.throw</code>, <code>.assert</code>, <code>.sout</code>, <code>.serr</code></td></tr>
    <tr><td>Expressions</td><td><code>.not</code>, <code>.par</code>, <code>.cast</code>, <code>.instanceof</code>, <code>.opt</code>, <code>.stream</code>, <code>.forEach</code></td></tr>
  </tbody>
</table>
<p>
  They follow the project's <strong>Java level</strong>: below Java 10, <code>.var</code> and <code>.for</code> leave a stop where the type goes instead
  of writing <code>var</code>. Rust gets its own postfix set from rust-analyzer, in the same list.
</p>

<h2>Members that do not exist yet</h2>
<p>
  Two families, one gesture: you are in a class body, you start typing a name, and the thing you mean has not been written.
</p>

<h3>The method this class calls and does not declare</h3>
<p>
  You wrote <code>randomico()</code> inside a method; now <code>rand</code> in the class body offers to declare it. The <strong>call site is the
  specification</strong>: the arguments' types and names become the signature, what the result is used as becomes the return type, and a call from a
  <code>static</code> method asks for a <code>static</code> one — the reading the <em>Create method</em> quick fix makes, so the popup and
  <kbd>Alt</kbd> + <kbd>Enter</kbd> cannot describe one member two ways.
</p>
<ul>
  <li>It is read from the <strong>tree</strong>, not from the diagnostic — which is what survives typing. A half-written name in a class body is a syntax
    error, and while it lasts its member is not validated, so the red mark under <code>randomico()</code> disappears exactly when the offer is wanted.
    The call is still in the tree.</li>
  <li>It reads the calls made on this class from <strong>elsewhere in the file</strong>, which a <strong>nested</strong> class needs:
    <code>c.randomico("ciao")</code> written in the outer class, on an instance of the inner one, makes <code>ra</code> in the inner class offer it — as
    <code>public</code>, since another class calls it. Only a receiver whose type resolves to the class you are in counts.</li>
  <li>A method the class <strong>inherits</strong> is not missing: the supertypes are walked first, and an unreadable hierarchy means silence.</li>
</ul>

<h3>The accessors a field is missing</h3>
<p>Typing <code>getCust</code> in a class body offers <strong><code>getCustomer()</code></strong>, and accepting writes the whole method:</p>
<pre><code>{@html highlightCode(`public String getCustomer() {
    return customer;
}`, 'java')}</code></pre>
<ul class="prop-list">
  <li><code>getX</code>or <code>isX</code> for a primitive <code>boolean</code> — what JavaBeans says and what reflecting frameworks look for</li>
  <li><code>setX</code>with <code>this.</code> on the assignment: the parameter shadows the field</li>
  <li><code>withX</code>the builder-style setter, assigning and returning the object so calls chain — the member <strong>Generate</strong> writes under "With"</li>
</ul>
<p>
  A <code>final</code> field is offered a getter and nothing else; a <code>static</code> one no <code>withX</code>, with no <code>this</code> to return.
  A field named <code>isActive</code> keeps its prefix and is set and chained by its property: <code>setActive</code>, <code>withActive</code>.
</p>
<dl class="meta-grid">
  <dt>As ghost text</dt>
  <dd>When exactly one candidate matches, the answer also appears ahead of the caret — <code>→ public String getCustomer() &lbrace; … &rbrace;</code> — written with <kbd>Tab</kbd>. Two candidates produce nothing, however close the second: ghost text reads like text already there, so being wrong costs trust.</dd>
  <dt>In the open popup</dt>
  <dd>The grey text previews the highlighted row, following the arrows — on <strong>one line</strong>, since the popup opens right under the caret. It is the popup's selection drawn, not a second proposal: <kbd>Tab</kbd> still belongs to the popup.</dd>
  <dt>With the popup closed</dt>
  <dd>After <kbd>Esc</kbd>, or with the auto-popup off, the proposal is drawn in full, and <kbd>Tab</kbd> writes it.</dd>
</dl>
<Callout variant="info" title="Quiet where it is not wanted">
  Only at a <strong>member position</strong> — not in a method body, where <code>getCustomer</code> is a call, and not after a dot; only for a field
  with <strong>no accessor already</strong>; and never on an empty prefix. <strong>Generate</strong> (<kbd>Alt</kbd> + <kbd>Insert</kbd>) is still the
  place for the whole set, with the style options — fluent accessors, snake_case names, <code>final</code> parameters. A completion takes the
  conventional form.
</Callout>

<h2>Generated members</h2>
<p>
  Plenty of Java members exist at compile time and nowhere in the source. Bennu models them, so completion, hover, find usages and the checks treat
  them like any declaration.
</p>
<div class="feature-grid two-col">
  <div class="feature-card">
    <div class="fc-eyebrow">From the language</div>
    <div class="fc-title">Records</div>
    <div class="fc-desc">An accessor per component — <code>p.x()</code>, not <code>getX()</code> — the backing fields, the canonical constructor, and <code>toString</code>, <code>equals</code>, <code>hashCode</code>. A member the record writes itself always wins.</div>
  </div>
  <div class="feature-card">
    <div class="fc-eyebrow">When the file imports Lombok</div>
    <div class="fc-title">Lombok</div>
    <div class="fc-desc"><code>@Getter</code>, <code>@Setter</code>, <code>@Data</code>, <code>@Value</code> accessors, <code>@With</code> copies, the <code>@Slf4j</code> <code>log</code>, the <code>@AllArgsConstructor</code> and <code>@RequiredArgsConstructor</code> constructors — on an enum with valued constants too — and <code>@UtilityClass</code>, making every member <code>static</code> and the class <code>final</code>.</div>
  </div>
</div>
<Callout variant="info" title="Lombok's members need Lombok's import">
  That is what makes the annotation mean anything — your own <code>@Data</code> in another package generates nothing. A record's members need no such gate.
</Callout>
<dl class="meta-grid">
  <dt><code>@Accessors</code></dt>
  <dd>At class or field level, the field's winning: <code>fluent = true</code> names both accessors after the field — <code>o.customer()</code> reads, <code>o.customer("x")</code> writes — and <code>chain = true</code>, which <code>fluent</code> turns on by itself, makes the setter return the object. The <code>prefix</code> element is not read yet.</dd>
  <dt><code>AccessLevel</code></dt>
  <dd>On <code>@Getter</code> and <code>@Setter</code>, at class or field level: <code>@Setter(AccessLevel.PACKAGE)</code> is a package-private setter; <code>AccessLevel.NONE</code> generates nothing. Generated constructors carry their <code>access</code> the same way, with the parameters Lombok gives them — <code>@RequiredArgsConstructor</code> takes the unassigned <code>final</code> fields and the <code>@NonNull</code> ones.</dd>
  <dt><code>@Builder</code></dt>
  <dd>The class it really generates: the static <code>builder()</code>, a setter per field returning the builder, <code>build()</code>, and with <code>toBuilder = true</code> the instance <code>toBuilder()</code>. <code>@Singular</code> adds the single-element adder — <code>.tag("a")</code>, named from the field or <code>@Singular("tag")</code> — and <code>clearTags()</code>. A <code>@Builder.Default</code> field is still a constructor parameter. On a <strong>constructor</strong> or a <strong>static factory</strong>, the builder takes that element's parameters, is named after what it builds, and <code>build()</code> returns that.</dd>
  <dt><code>@Value</code>, <code>@FieldDefaults</code></dt>
  <dd><code>@Value</code> makes the fields <code>private final</code> and the class <code>final</code>, though the source writes none of it. <code>@FieldDefaults(makeFinal = true, level = …)</code> does the same, with <code>@NonFinal</code> and <code>@PackagePrivate</code> honoured.</dd>
  <dt><code>@Delegate</code>, <code>@SuperBuilder</code></dt>
  <dd>Their members depend on a type that cannot be read while indexing, so the type is marked as having <strong>more members than the list</strong> — nothing concludes "no such method" from their absence, and the rest of it is checked as before.</dd>
</dl>
<p>A primitive <code>boolean</code> field already starting with <code>is</code> keeps it, as Lombok does:</p>
<table>
  <thead><tr><th>Field</th><th>Getter</th><th>Setter</th></tr></thead>
  <tbody>
    <tr><td><code>boolean isRunning</code></td><td><code>isRunning()</code></td><td><code>setRunning(…)</code></td></tr>
    <tr><td><code>boolean is_attivo</code></td><td><code>is_attivo()</code></td><td>—</td></tr>
    <tr><td><code>boolean isattivo</code></td><td><code>isIsattivo()</code> — what follows <code>is</code> is lowercase</td><td>—</td></tr>
    <tr><td><code>Boolean active</code></td><td><code>getActive()</code> — a wrapper is a plain getter</td><td>—</td></tr>
  </tbody>
</table>
<Callout variant="warning" title="Go-to on a generated member">
  There is no name in the source to jump to, so it has nothing to open. Go-to on the backing <em>field</em>, or a record's component, works.
</Callout>
