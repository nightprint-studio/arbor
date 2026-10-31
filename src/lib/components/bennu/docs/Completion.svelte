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
  <li>Typing fast does not send a question per key. Within a burst the list is asked for when you pause, and at least every few hundred
    milliseconds while the burst lasts; the first letter, a <code>.</code> and <kbd>Ctrl</kbd> + <kbd>Space</kbd> are answered at once.</li>
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
    type, an assignment to something typed, a <code>return</code> — a lambda's too — and a condition, which wants a <code>boolean</code> and
    turns a member list into its predicates. The exact type comes before a subtype, and the only exact match is preselected: <code>return Ra</code>
    in a method returning <code>RawIdentity</code> opens on <code>RawIdentity</code>, <code>return builder.</code> on <code>build()</code>, a bare
    <code>return</code> on the local or field of that type. With a generic return, <code>Optional&lt;RawIdentity&gt;</code>, the element type follows
    the outer one. A <code>void</code> method sinks where a value is wanted. It <em>ranks</em>, never filters: hiding the misses would hide the
    right answer whenever the type is not the one expected.</li>
  <li><strong>Who declares it.</strong> After a dot, what the receiver's own type declares comes first, then what it inherits — the nearest
    supertype first — then <code>java.lang.Object</code>'s, last. A record's components come before the <code>equals</code>,
    <code>hashCode</code> and <code>toString</code> the compiler writes for it. Only the position's type and how literally your letters match
    come before this: <code>route.</code> opens on <code>prefix()</code>, preselected, while <code>route.to</code> still offers
    <code>toString</code> above a declared <code>targetOrigin</code>.</li>
  <li><strong>What you picked last time.</strong> A fact about you, not the code: on a <code>List</code> you reach for <code>stream</code>,
    not <code>listIterator</code>. Kept per <em>declaring type</em>, so what is learned about <code>java.util.List</code> carries to every
    receiver inheriting it. Frequency and recency both count, so a fresh choice can overtake an old habit and one stray pick cannot bury a
    name you use constantly. It lives for the session and is never written to disk — a persisted ranking that drifted would be invisible.</li>
  <li><strong>What the receiver is.</strong> After a <em>type</em> — <code>Color.</code> — the statics are the answer and a constant goes to
    the very top, instance members to the bottom. Through a <em>value</em> — <code>color.</code> — the other way round, more gently: a
    static through an instance compiles, it is just rarely meant. After a type the popup also offers <code>class</code> — <code>Order.class</code>
    — which no class declares but every type has; where a <code>Class&lt;…&gt;</code> is wanted it ranks first. The same goes for what the
    language gives without declaring it: an array offers <code>length</code> and <code>clone()</code> (and the methods of <code>Object</code>,
    not those of its element type); a class name you are inside of offers <code>this</code> and <code>super</code>
    (<code>Outer.this</code>), and a direct superinterface offers <code>super</code> (<code>Iface.super.run()</code>); and
    <code>new Foo(</code> on a class with no constructors shows the empty one. Inside a lambda, an argument that nothing of a known
    type fits is drawn as the lambda's own parameter (<code>x -&gt; tag(</code> → <code>x</code>); a parameter starting with
    <code>_</code> is never used.</li>
  <li><strong>Deprecated.</strong> Still offered, since you may be reading old code, but last — on your own source, since library members
    carry no annotations in compiled form.</li>
  <li><strong>What this file already uses.</strong> A name already in the buffer is likely wanted again. It decides between otherwise-equal
    candidates, and cannot do more.</li>
  <li><strong>The name being filled.</strong> <code>String customerName = c.</code> has three <code>String</code> members and the alphabet puts
    <code>email</code> first; the declaration says <code>name</code>. A candidate whose words echo the hole's rise — <code>getName</code> reads as
    <code>name</code>, <code>orders</code> as <code>order</code>, a shared tail or head counts less than the whole name. In an argument slot the
    hole is named by the callee: <code>findByCustomerId(</code> lifts <code>customerId</code>.</li>
  <li><strong>What the project does.</strong> How often the project calls each method, and which method it calls <em>after</em> the previous one in
    the chain — <code>close()</code> after <code>open()</code>, on the same variable even when the calls are statements apart. Counted while
    the index builds, as a band (each step a doubling, a name used once counts for nothing), never written to disk.</li>
</ol>
<p>
  The last two, and the shape of the code below, are <strong>Smart suggestions</strong> in Settings → Completion: each is a switch, and a
  strength scales the learned ones. They only add to a score — nothing is hidden — and none can overrule the type the position wants.
</p>

<h3>Smart suggestions</h3>
<dl class="meta-grid">
  <dt>Chains</dt>
  <dd>When the position wants a type, members of members that produce it are offered below the direct fits: <code>String name = order.</code> also
    lists <code>getCustomer().getName()</code>. Zero-argument calls only, one level deep, through your own classes; a member that already
    fits is not chained through, and a call that is already written gets none.</dd>
  <dt>Builders</dt>
  <dd>A setter already called in the chain sinks: after <code>Order.builder().customer(c).</code> the next property or <code>build()</code> leads, not
    <code>customer</code> again.</dd>
  <dt>Unused locals</dt>
  <dd>A local declared and not used yet rises: it is very probably what comes next.</dd>
  <dt><code>catch (</code></dt>
  <dd>The exceptions the <code>try</code> above can throw come first — from <code>throw new</code> and from the <code>throws</code> clause of each call it
    makes (the first few calls; one that cannot be bound to a single overload says nothing). A multi-catch leaves out the alternatives it
    already lists, and a second <code>catch</code> reads the same <code>try</code>.</dd>
</dl>
<p>
  Keywords come below anything the index resolved. Words <em>scraped from the buffer</em> are offered only when the index answered nothing —
  before it finished building, or in a file no project owns: they are a regular expression over the text, and beside resolved names they bury
  them under look-alikes. How well what you typed matches still decides between neighbours.
</p>

<h3>Finishing the line</h3>
<p>
  Beside the popup, Bennu can draw the <strong>rest of the line</strong> in grey after the caret: <kbd>Tab</kbd> writes it, <kbd>Esc</kbd> dismisses it.
  It has several sources, and all but the last need no model.
</p>
<dl class="meta-grid">
  <dt>Log statements</dt>
  <dd>The project's log calls are counted once, in the background, as a counter per <em>pattern</em> — never a list of the calls. A pattern is the level
    plus a template in which the logger, the method's name and parameters, and the caught exception are slots, and a counter is kept for the three
    places a log goes: the first statement of a method, inside a <code>catch</code>, anywhere else. Start a log call and, when one pattern clearly leads
    its place, the rest of the statement is drawn with the slots filled from your method: <code>log.debug("START - rename(&#123;&#125;, &#123;&#125;)", from, to);</code>.
    A project with no house style gets nothing. A saved file replaces its own contribution, so counters follow the code. The counters are saved in your Arbor data folder (templates and logger names only, never source) and updated when <code>.java</code> files change on disk while the project tree is open.</dd>
  <dt>Lines the project repeats</dt>
  <dd>Lines that at least two files write alike are remembered (the text of those only, in memory, never on disk). When the start of your line is
    one that other files all finish the same way — at least two files, and four in five of the continuations agree — the rest is drawn from them,
    ahead of the model: <code>return repository.findOr</code> ends the way the project always ends it. Your own file does not count in its own favour,
    and a saved file replaces what it contributed. It is part of the same switches as the log statements and the model.</dd>
  <dt>The body of a sibling method</dt>
  <dd>After the <code>&#123;</code> of a method (or on the empty line below it), if another method has a similar name and the same parameters, its body is drawn with the words that differ replaced
    (<code>findByCodiceCliente</code> → <code>findByCodiceFornitore</code>, in names, parameters and types). It is held in memory only and never saved. If the new body would use a name that
    exists nowhere in the project, it is not drawn.</dd>
  <dt>Annotations the project always writes</dt>
  <dd>At the start of a member, after <code>@PostMapping(</code>, the arguments the project writes for that annotation are drawn (the text of the strings left empty), without the elements you already typed; on the empty line under an annotation, the annotation that usually comes next is drawn in the layout the project uses. Held in memory only, never saved, and only when at least three uses agree to the tune of 60%.</dd>
  <dt>The usual argument</dt>
  <dd>In a call's parentheses, when other files always pass the same argument at that place (at least three, and most of them), its text is drawn after the caret, or completes what you have typed
    (<code>filter(Boo</code> → <code>Boolean.FALSE::equals</code>), and the completion list puts it first. A local variable or parameter inside the argument is not part of the habit: <code>new Factory().create(v1)</code> and <code>new Factory().create(v2)</code> are counted as one, <code>new Factory().create($0:Order)</code> (the slot is the variable's declared type), and drawn with the nearest variable in scope of that type; with none, nothing is drawn. The <em>What Bennu has learned</em> window shows the template, and hiding a row hides the template. Held in memory only, never saved. Where the parameter is a type variable (<code>.field(helper(</code> with <code>&lt;T&gt; Matcher&lt;T&gt; helper(T value)</code>), the call around it decides what <code>T</code> is: a usual argument of another type (a constant of the wrong enum) is not offered, and when nothing settles <code>T</code> it is drawn fainter and listed after the other rows.</dd>
  <dt>What you pick</dt>
  <dd>After a type (<code>list.</code>) or in an argument of a call, the entries you chose at least twice there are put first in the list, the one you chose most often preselected when it was at least three picks and half of them.
    Only the names and the counts are kept, in your Arbor data folder; the switch is <em>Count how often suggestions are taken</em>, and <em>Forget</em> deletes them.</dd>
  <dt>What Bennu remembers</dt>
  <dd>Argument habits, annotation shapes and variable names are saved per project in your Arbor data folder (short pieces of code, never in a repository, never sent anywhere), and where a project has seen too little they
    can be borrowed from your other projects. Two switches in Settings → Completion turn the two parts off, and <em>Forget</em> deletes everything. Where two variables fit an argument equally, the one your project usually passes
    at that place is drawn.</dd>
  <dt>Argument lists</dt>
  <dd>Right after the <code>(</code> of a call, the arguments are drawn all at once when each parameter has a clear match among the names in scope (same type, a name like the
    parameter's, or the only name of that type) or a clear constant of its enum. A parameter nothing in scope fits gets a value that merely compiles (<code>null</code>, <code>0</code>, <code>false</code>, <code>""</code>) as long as at least one other parameter has a real match. When several names fit a parameter equally (two variables of the same type, a parameter that resembles neither), the one declared first is drawn as a guess. With the editor's own <code>)</code> already
    there, only the arguments are drawn.</dd>
  <dt>The local model</dt>
  <dd>A small model that runs on your machine, in the background, and answers nothing while it is busy or still loading. After a <code>.</code> it is
    constrained to the member names the index offers there, so it cannot invent one. It is a guess, so it is <strong>off until you turn it on</strong>.
    It stays silent after a finished annotation, statement or block, and a line that is only a type (<code>Order</code>, <code>final List&lt;Item&gt;</code>)
    gets a variable name (<code>order</code>, <code>items</code>) instead of a call. A line that names a type, method or variable the file and
    project do not have, a ternary, or a continuation that is not a real word is thrown away, and a missing space after <code>=</code>, <code>return</code> or <code>if</code> is added.
    The word you are typing is not shown to the model but forced as the start of its answer, so a half-typed word does not confuse it,
    and a new line under an unfinished call chain (a builder) gets the next step without typing anything.
    A declaration is never offered where an argument goes, and a member the receiver does not have (a builder method after
    <code>.build()</code>, a method an <code>&amp;&amp;</code> operand does not have) is checked against the index: the suggestion is cut before it, or dropped.
    While the model writes a chain, the name after each dot is limited to what the receiver written so far has, so a method that does not exist is not written in the first place.
    While the completion list is open, the selected row shows what it adds in grey, so a list with one obvious row still shows its outcome.</dd>
</dl>
<p>
  Settings → Completion → <strong>Model suggestions</strong> has the switch, the folder with the model files (a personal copy there gives you a model
  adapted to your own code), how sure the model has to be before it is drawn, a switch for the log statements, <strong>Count how often suggestions are taken</strong> (only how many suggestions of each kind were shown, taken with Tab and dismissed, per confidence level, in your profile; no code is stored and nothing is sent: when the model's suggestions are taken at least half the time above some confidence, the page offers that value and changes it only when you press the button), <em>What Bennu has learned…</em> (command palette, or the <strong>Open…</strong> button in Settings › Completion › Learning) lists what the completion counted and lets you stop offering a row or forget a counter of your own. Where a parameter is a <code>Class&lt;T&gt;</code> the list opens on the usual classes already written as <code>String.class</code>, and on the project's implementors where <code>T</code> is bounded; after a comma, the remaining arguments are drawn by their types. Where a call always takes a call on some variable (<code>new ByteArrayInputStream(bao.toByteArray())</code>), the ghost text draws it with the nearest variable in scope that has that method. On an empty line, after a statement the project always follows the same way, the start of the usual next call is drawn (<code>jaxb_processor.marshal(</code>); only the start, since the arguments are filled by the other layers. <strong>Remember what Bennu learned in each project</strong> also lets suggestions follow how the project is laid out: the space after <code>if</code>/<code>for</code>, before a brace and after a comma are measured from its files and used where the project is consistent, never inside string literals. <strong>Ignore commented-out code</strong> (off by default: blocks of code in <code>//</code> lines are left out of what the model reads, so it does not copy a version you commented out; explanations in comments are still read), Where a project creates the same type under a name nearly every time (<code>bao</code> is always a <code>ByteArrayOutputStream</code>), writing <code>val bao</code> draws the rest of the statement, and the popup after <code>new</code> lists that type first; it is listed under <em>Types by variable name</em> in <em>What Bennu has learned…</em>. <strong>Learn from corrections</strong> (off by default: after you take a suggestion it notes whether you left it, cut its tail, wrote on, changed a part, only re-lay it out — spaces, line breaks, indentation, which is not a change of content — or took it back, once you have moved on; a part you change and put back as it was counts as left alone; counters per kind of suggestion, in your profile, nothing sent; a kind you rarely keep is drawn with up to a third less confidence, so it shows less often; <em>Keep the text too</em> adds a log of the texts capped at 1 MB; <em>Notify when it learns</em> shows each counted correction as a notification, with the kind of suggestion, where, what happened and how many of that kind were seen — for checking what is learned; <em>Forget</em> deletes both), <strong>Check with Bennu</strong> (the model's line is run through the code checks of the method you are in and dropped when it calls a method the receiver does not have, passes the wrong arguments or uses a variable that does not exist; a class you have not imported yet does not count against it, nor does a statement that is not finished), and <strong>Try three candidates</strong>: the model
  gives three answers and the first that passes the checks is drawn, instead of nothing when its only answer fails. It is several times slower (about 280 ms against 60 in the lab), so it is off by default. The palette has
  <em>Model suggestions: turn on / off</em>.
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
  <li>A class picked <strong>after <code>new</code></strong> writes the call too: <code>ArrayList</code> becomes <code>ArrayList&lt;&gt;()</code> (the
    diamond only for a generic class), with the caret after it when a constructor takes nothing and between the parentheses when every one
    takes something. An <strong>interface or an abstract class</strong> writes <code>Runnable() &#123;&#125;</code> with the caret inside the braces — an anonymous class —
    except an abstract class that can only be built with arguments, where the caret waits between the parentheses. A name that stands for
    several classes (<code>Shape</code>, in <code>java.awt</code> and elsewhere) keeps just the name until you choose its import, and an enum is not a thing you create.</li>
  <li>An <strong>overloaded</strong> method is <em>one row per overload</em>, fewest parameters first — <code>wait()</code>,
    <code>wait(long timeoutMillis)</code>, <code>wait(long timeoutMillis, int nanos)</code> — since each leaves the caret in a different place.
    After <code>::</code> they are one row, because a method reference writes only the name. A method that merely <strong>overrides</strong> an
    inherited one appears once.</li>
  <li>What could not be written there is not offered: a <code>private</code> member of another class, a package-private one from another package,
    and a <code>protected</code> one through a receiver outside its package and your class — <code>route.clone()</code> and
    <code>route.finalize()</code> are not Java, <code>this.clone()</code> is.</li>
</ul>

<h2>Reading a row</h2>
<dl class="meta-grid">
  <dt>The icon</dt>
  <dd>What it <strong>is</strong>. A member is a circle — <code>m</code> method, <code>c</code> constructor, <code>f</code> field,
    <code>v</code> variable, <code>p</code> parameter — and a type a rounded square — <code>c</code> class, <code>i</code> interface,
    <code>e</code> enum, <code>r</code> record, <code>@</code> annotation; a package is a folder, a template <code>{'{}'}</code>. A dot in the
    corner marks <em>static</em>, a lock <em>final</em>, a dashed outline <em>abstract</em>, a diagonal stroke <em>deprecated</em>.</dd>
  <dt>The name</dt>
  <dd>What it is <strong>called</strong>, with the letters you typed marked. <strong>Bold</strong> when the receiver's own type declares it,
    regular when inherited, dimmed when it is <code>java.lang.Object</code>'s. A deprecated one is struck through.</dd>
  <dt>The parameters</dt>
  <dd>Beside the name, with their names where the member carries them: <code>equals(Object obj)</code>. A library compiled without
    parameter names shows the types alone.</dd>
  <dt>The right edge</dt>
  <dd>What a method <strong>returns</strong> or a field holds — <code>getClass()</code> as <code>Class&lt;? extends ServiceRoute&gt;</code>,
    typed after the receiver — and, for an inherited member only, where it <strong>comes from</strong>: what tells <code>List.of</code> from
    <code>Set.of</code>.</dd>
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
    <tr><td>Modifiers</td><td><code>pf</code>, <code>prf</code>, <code>psf</code>, <code>psfi</code>,<code>psfs</code>, <code>prsf</code>, <code>prsfi</code>, <code>prsfs</code>, <code>psvm</code> for a <code>main</code></td></tr>
    <tr><td>Printing</td><td><code>sout</code>, <code>souf</code>, <code>serr</code></td></tr>
    <tr><td>Statements</td><td><code>fori</code>, <code>ifn</code>, <code>inn</code>, <code>thr</code>, <code>iter</code></td></tr>
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
<p>
  <code>iter</code> is the one that <em>reads the surrounding code</em>: it loops over the nearest array or <code>Iterable</code> local or parameter
  in scope, with the element type and a name spelled the project's way (<code>var</code> from Java 10). It is offered only where such a variable
  exists, at the start of a statement, and its row says which variable it loops over.
</p>
<Callout variant="info" title="What is deliberately missing">
  Abbreviations that would have to <em>guess</em> — a version that guesses writes a name that is not there. <code>iter</code> is offered only when
  there is something to iterate.
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
<p>
  What a template writes depends on the <strong>type</strong> of the expression, and a template is only listed where it applies:
  <code>.for</code> on a collection or an array, <code>.if</code> on a boolean, <code>.nn</code> on a value that can be <code>null</code>,
  <code>.ifpe</code> on an <code>Optional</code>. The names it proposes come from the code, spelled in the project's naming convention for local variables or, where none is set, in the one the
  file's names follow — <code>orders.for</code> declares an <code>order</code>
  of the element type — and the imports its text needs are added with it. Templates appear once the type is known and you have started typing a name
  after the dot.
</p>
<table>
  <thead><tr><th>Family</th><th>Templates</th></tr></thead>
  <tbody>
    <tr><td>Null checks</td><td><code>.nn</code> / <code>.notnull</code>, <code>.null</code>, <code>.reqnonnull</code></td></tr>
    <tr><td>Conditions</td><td><code>.if</code>, <code>.else</code>, <code>.while</code>, <code>.not</code>, <code>.assert</code>, <code>.switch</code></td></tr>
    <tr><td>Loops</td><td><code>.for</code> / <code>.iter</code>, <code>.fori</code>, <code>.forr</code>, <code>.forEach</code>, <code>.stream</code></td></tr>
    <tr><td>Optional</td><td><code>.opt</code>, <code>.optof</code>, <code>.ifp</code>, <code>.ifpe</code>, <code>.ifget</code>, <code>.orThrow</code>, <code>.orGet</code></td></tr>
    <tr><td>Statements</td><td><code>.var</code>, <code>.return</code>, <code>.throw</code>, <code>.try</code>, <code>.twr</code>, <code>.synchronized</code>, <code>.sout</code>, <code>.soutv</code>, <code>.souf</code>, <code>.serr</code></td></tr>
    <tr><td>Expressions</td><td><code>.inst</code> / <code>.instanceof</code>, <code>.cast</code>, <code>.castvar</code>, <code>.par</code>, <code>.arg</code>, <code>.lambda</code>, <code>.format</code>, <code>.new</code> on a class name</td></tr>
  </tbody>
</table>
<p>
  They follow the <strong>Java level</strong> of the file's module, and write what that level compiles rather than disappearing:
</p>
<table>
  <thead><tr><th>Template</th><th>Below</th><th>From</th></tr></thead>
  <tbody>
    <tr><td><code>.var</code>, <code>.for</code>, <code>.twr</code></td><td>the declared type, <code>Order order</code></td><td>Java 10: <code>var order</code></td></tr>
    <tr><td><code>.inst</code></td><td><code>instanceof</code> and a cast into a local</td><td>Java 16: a type pattern</td></tr>
    <tr><td><code>.ifpe</code></td><td><code>if (…isPresent()) … else …</code></td><td>Java 9: <code>ifPresentOrElse</code></td></tr>
    <tr><td><code>.stream</code> on an <code>Optional</code></td><td><code>map(Stream::of).orElseGet(Stream::empty)</code></td><td>Java 9: not listed, <code>stream()</code> is its member</td></tr>
  </tbody>
</table>
<p>
  An expression that does work is never written twice: where the older form needs the value in two places, <code>repo.findById(id).ifget</code>
  declares a local for it first, and <code>.fori</code>, whose bound runs on every iteration, is not offered on a call. Templates with no equivalent
  at the level — <code>Optional</code> before Java 8, <code>.twr</code> before 7 — are not listed. Rust gets its own postfix set from rust-analyzer, in
  the same list.
</p>
<p>
  Postfix templates of your own sit beside them — a template of the <strong>Postfix</strong> kind, offered on the values it says it applies to. See
  <strong>Code templates</strong>.
</p>

<h2>After a dot</h2>
<p>
  The list is the members of the value on the left, and its postfix templates — nothing else: no keywords, no words from the file, no local names. When
  the value's type cannot be worked out, the popup says there are no suggestions rather than offering what cannot follow a dot.
</p>

<h2>Method references</h2>
<p>
  After <code>Type::</code> or <code>value::</code> the list is the methods a reference can name, and <code>new</code> for a class that can be
  instantiated. The ones that fit the function expected there come first — in <code>optional.map(ResolvedIdentity::|)</code>, the instance methods with
  no parameters and the static methods taking a <code>ResolvedIdentity</code> — and accepting one writes only the name.
</p>

<h2>Functions passed as arguments</h2>
<p>
  Inside a call that takes a function, the type the function receives is offered first: in <code>identity_resolver.resolve_identity().map(|)</code> it
  is <code>ResolvedIdentity</code>, the value inside the <code>Optional</code>. <kbd>Ctrl</kbd> + <kbd>Space</kbd> with nothing typed opens on it,
  preselected, with its import.
</p>

<h2>A builder does not repeat itself</h2>
<p>
  After <code>Device.builder().name("a").</code> the methods the chain has already called, and that return the same type (the fluent setters), move to the end of
  the list: <code>id</code>, <code>type</code> and <code>build</code> come first. Nothing is removed, and with a prefix typed the order is the usual one.
</p>

<h2>Annotation values and configuration keys</h2>
<p>
  Inside an annotation's parentheses the list opens on the values the other files write for that element (<code>produces = MediaType.APPLICATION_JSON_VALUE</code>,
  <code>"/orders"</code>), most used first. Inside <code>@Value("&#36;&#123;…")</code>, <code>@ConfigurationProperties(prefix = "…")</code> and
  <code>@ConditionalOnProperty(name = "…")</code> it offers the keys of <code>application*.properties|yml|yaml</code> and <code>bootstrap*</code>, nested YAML
  flattened with dots, the nearest module's first. The first request in a session may find nothing while the project is read once in the background.
</p>

<h2>Mapping two classes</h2>
<p>
  In a method that copies one object into another, the empty line after the construction or after a mapping line offers the next property of the target that no line
  assigned yet, taken from the source by name and type (equal, equal ignoring case, <code>_</code> and plurals, or <code>id</code> against <code>customerId</code>) and written in the
  style the lines above use (<code>d.setX(e.getX());</code> or <code>.x(e.getX())</code>). A type that would need a conversion, or two sources that fit equally, are left to you.
</p>

<h2>The class an argument takes</h2>
<p>
  In <code>service.list(C|)</code> the type names open on the class the parameter takes, preselected, with its import, ahead of names that merely start alike. When overloads
  disagree about the slot, the usual order stands. After <code>EnumType.TI|</code> the constants that share a word with the names around the caret rise above the others.
</p>

<h2>The constants of the class a position wants</h2>
<p>
  Where the code wants a class — <code>Money m = |</code>, a parameter of that class — its constants and no-argument static factories (<code>Money.ZERO</code>, <code>Money.zero()</code>,
  <code>Optional.empty()</code>) are offered first, with their import. Only members that return exactly that class; <code>String</code>, the boxed types and <code>Object</code> are left out, and an enum has its own list below.
</p>

<h2>An argument that takes an enum</h2>
<p>
  When the call has one overload at that position (a builder's <code>type(|)</code>), its parameter is the enum and the list opens on that enum's
  constants, written <code>DeviceType.PIN</code> with their import. Each constant is split into words (<code>TIPO_PIN</code> → <em>tipo</em>,
  <em>pin</em>, the enum's own name left out) and compared with the words around the caret: a field called <code>pin</code> in the file or
  a method <code>connectPin</code> puts <code>PIN</code> first, preselected when it is the only best fit. Overloads that disagree offer no constants.
  The same holds wherever the surrounding code fixes the enum: the other side of <code>==</code> / <code>!=</code>, a ternary branch, an array element, <code>equals(…)</code>,
  <code>List.of(…)</code>, a declared variable's initializer and a constructor argument. In a <code>case</code> label the constants appear bare, without those an earlier label already names.
</p>

<h2>Keywords by position</h2>
<p>
  The language's keywords are offered by position: at the start of a statement the statement words (<code>return</code> first in a method that returns a value, after <code>if</code>/<code>for</code>/<code>while</code>
  in one that does not), in a class body the member modifiers, at the top of the file <code>package</code>, <code>import</code> and the type declarations, and after <code>=</code> or <code>(</code> only the words an
  expression can start with. <code>else</code>, <code>catch</code> and <code>finally</code> appear only right after the block they continue.
</p>

<h2>Arguments by type and by name</h2>
<p>
  In <code>transfer(|)</code> the parameters, locals and fields of the class whose type fits the slot come first, those whose name looks like the
  parameter's own name before the others (<code>from</code> leads the slot called <code>from</code>), and a clear winner is preselected. When
  overloads disagree about the slot, or after a <code>.</code>, the list is the usual one.
</p>

<h2>A declaration's name</h2>
<p>
  After the type of a declaration — a field after its modifiers, a local variable, a method or constructor parameter, a <code>for</code> or
  <code>try</code> resource — the name IntelliJ would suggest is drawn ahead of the caret, and <kbd>Tab</kbd> writes it:
</p>
<table>
  <thead><tr><th>Written</th><th>Suggested</th></tr></thead>
  <tbody>
    <tr><td><code>private final OrderRepository </code></td><td><code>orderRepository</code></td></tr>
    <tr><td><code>List&lt;Order&gt; </code>, <code>Order[] </code></td><td><code>orders</code></td></tr>
    <tr><td><code>Optional&lt;Order&gt; </code></td><td><code>order</code></td></tr>
    <tr><td><code>Class&lt;?&gt; </code></td><td><code>clazz</code></td></tr>
  </tbody>
</table>
<p>
  The name is spelled the project's way: with the convention its naming rules set for fields, local variables or parameters, and where they set none,
  with the one the file's names already follow — among <code>identity_resolver</code> fields, <code>private final FilterConfigurator </code> suggests
  <code>filter_configurator</code>.
</p>
<p>
  A name already declared where Java would refuse a second one gets a digit (<code>order1</code>). Once you start typing the name, the suggestion stays
  only while it continues what you typed. It never appears after <code>return</code>, <code>new</code> or an operator, inside arguments, strings or
  comments, or in a class header.
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
  <li>A <strong>method reference</strong> asks as surely as a call: after <code>.map(this::toIdentity)</code>, <code>toId</code> in the class body offers
    <code>toIdentity</code> with the signature of the functional interface it is passed as — and draws it as ghost text when it is the only
    match.</li>
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

<h2>Arguments and the editor's closing parenthesis</h2>
<p>
  The closing <code>)</code> the editor adds when you type <code>(</code> does not silence the ghost text: <code>@PostMapping(value = |)</code>
  and <code>foo(a, |)</code> are completed and the parenthesis stays where it is. Inside an annotation, <code>value = </code> is completed with the
  value the project writes for that element. After you pick a method from the list, its arguments follow the overload you picked. <code>list.toArray(</code>
  draws <code>Item[]::new</code> when the list's element type is known, and where the project passes several different lambdas or constants to
  a method, the start they share (<code>it -&gt; </code>, <code>Boolean.</code>) is drawn.
</p>
<p>
  Where an argument or a declaration fixes a class, the ghost text writes how the code makes one: a method of the same file that returns it
  (<code>tabellato(</code>), otherwise a static factory (<code>Pair.of(</code>, <code>List.of(</code>), otherwise <code>new Foo(</code>.
  A generic parameter is settled from the arguments already written and from what the call around it wants: after
  <code>Collections.addAll(items, </code> the ghost text draws the element type of <code>items</code> (<code>Pair.of(</code>), and inside that call
  the type its second argument must have (<code>List.of(</code>).
</p>
<p>
  After a comma, if the element before it is a call, the same call is drawn again with its string literals emptied and a number held in a
  string counted up: <code>tabellato("1", "ITALIA"), </code> → <code>tabellato("2", "")</code>.
</p>

<h2>Animated popup and ghost text</h2>
<p>
  Settings → Completion → <em>Animate the popup and ghost text</em> (off by default) fades the list in when it opens and uncovers ghost text
  from left to right as it appears. The text is whole from the first instant, so <kbd>Tab</kbd> takes the entire line even halfway through.
  Typing along a proposal shortens it without replaying the effect. It follows the app's animation setting and does nothing when animations are off.
</p>

<h2>Ghost text for certain continuations</h2>
<p>
  Beside the accessors above, two more continuations are drawn ahead of the caret, because each can only be one thing. <kbd>Tab</kbd> writes it;
  anything else you type replaces it, and when there is any doubt nothing is drawn. Settings → Completion → <em>Ghost text for certain
  continuations</em> turns both off.
</p>
<dl class="meta-grid">
  <dt>A caught exception's name</dt>
  <dd><code>catch (IOException&nbsp;</code> draws <code>e</code>, spelled the project's way, and <code>e1</code> when <code>e</code> is already in scope —
    a catch parameter may not shadow a local. Qualified types, <code>final</code> and a multi-catch are read; nothing is drawn once a name is started.</dd>
  <dt>The next <code>case</code></dt>
  <dd>On an empty line after the last arm of a <code>switch</code> over an enum, the first constant not yet covered, in declaration order, written in the
    style of the arms already there (<code>case X:</code> or <code>case X -&gt;</code>). Not drawn when there is a <code>default</code>, a pattern or
    <code>null</code> label, an arm that falls through, or a selector whose enum cannot be read.</dd>
</dl>

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
