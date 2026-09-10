<!-- Bennu docs — completion: what the editor offers as you type, and where it gets it. -->
<h1>Completion</h1>
<p class="doc-lead">
  What the popup knows, where each suggestion comes from, and the members that exist without
  being written anywhere.
</p>

<h2>The order things are offered in</h2>
<p>
  What the popup shows first is worked out from what the engine already knows, and it decides most
  of what completion feels like. Five things count, strongest first:
</p>
<ul>
  <li><strong>What the position wants.</strong> The strongest of the lot, and the only one that is
    about the <em>hole</em> rather than about the candidate. In <code>String name = order.</code>
    there are forty members on <code>order</code> and a handful that can be written there at all —
    and nothing about <code>order</code> says which, because the constraint is on the left of the
    <code>=</code>. It is read from four places: a declaration with a written type, an assignment
    to something already typed, a <code>return</code>, and a condition (which wants a
    <code>boolean</code>, and turns a member list into the predicates on it). A method returning
    <code>void</code> sinks where a value is wanted. It <em>ranks</em>, it does not filter: the
    match is by name, so a genuine subtype is a miss, and hiding on a miss would hide the right
    answer.</li>
  <li><strong>What you picked last time.</strong> Every other term is a fact about the code; this
    one is a fact about you — that on a <code>List</code> you reach for <code>stream</code> and not
    for <code>listIterator</code>. It is kept per <em>declaring type</em>, so what is learned is
    "reaching into <code>java.util.List</code>, you pick this", and it transfers to every receiver
    that inherits it. Frequency and recency both count: a fresh choice can overtake an old habit,
    and one stray acceptance cannot bury a name you use constantly. It lives for the session and is
    never written to disk — a persisted ranking that had drifted would be invisible, and nobody
    would think to clear a cache to fix a completion order.</li>
  <li><strong>What the receiver is.</strong> After a <em>type</em> name — <code>Color.</code> — the
    static members are the answer and the instance ones are not a program at all, so they go to the
    bottom; a constant, which is what a type is most often reached for, goes to the very top.
    Through a <em>value</em> — <code>color.</code> — it is the other way round and more gently: a
    static reached through an instance compiles, it is just rarely what was meant.</li>
  <li><strong>How far up the hierarchy it was found.</strong> A method the receiver's own class
    declares beats one it inherited, and the further up, the weaker.</li>
  <li><strong><code>java.lang.Object</code>.</strong> Its members match every prefix on every
    receiver and are hardly ever what you are reaching for, so they sink furthest —
    <code>list.</code> opens on <code>add</code>, not on <code>clone</code> and
    <code>equals</code>.</li>
  <li><strong>Deprecated.</strong> Still offered, since it exists and you may be reading old code,
    but last. Visible on your own source rather than on library members, which carry no annotations
    in compiled form.</li>
  <li><strong>What this file already uses.</strong> A name already written in the buffer is likely
    the one you want again. It decides between otherwise-equal candidates and cannot do more than
    that.</li>
</ul>
<p>
  <strong>Type names get two ranking terms of their own</strong>, because a simple name usually
  resolves to several types and nothing above can tell them apart — the typed letters are the same
  for all of them.
</p>
<p>
  The first is <strong>what this file has already said</strong>, and nothing outranks it: a type the
  file imports is the one it means, then a package it wildcard-imports, then its own package, which
  needs no import at all.
</p>
<p>
  The second is <strong>what the project imports</strong> — every <code>import</code> statement in
  the codebase is somebody choosing one of those candidates, and the aggregate of those choices is
  the best predictor of the next one. It is weighed against <strong>distance</strong>: the nearest
  package by shared prefix, so a sibling beats a cousin and both beat a stranger, with the JDK ahead
  of the rest of the classpath — not because it is special, but because a name matching both a JDK
  type and something in a jar you have never opened is almost always the JDK one.
</p>
<p>
  The two are weighed rather than ordered, and the balance is the point. A
  <code>java.util.List</code> written in four hundred files beats an
  <code>it.acme.model.List</code> nobody has ever imported, even though the second one is nearer.
  A type imported <em>once</em> somewhere does not beat anything: that is not evidence, it is a
  coincidence.
</p>
<p>
  <strong>Settings → Completion → Order type names by what this project imports</strong> turns the
  second term off, and the counts then stop being consulted at once rather than at the next rebuild.
  What the file itself imports still comes first — that was never a statistic.
</p>
<p>
  Keywords are offered below anything the index resolved. Words <em>scraped out of the buffer</em>
  are offered only when the index answered nothing at all — before it has finished building, or in
  a file no project owns. They are a regular expression over the text, which is the answer an
  editor with no index gives, and offering them beside resolved names buries the resolved ones
  under look-alikes. Beyond that, how well what you typed matches still decides between
  neighbours — the ordering is a starting point, not an override.
</p>

<h2>How a name is matched</h2>
<p>
  <strong>The name does not have to be spelled the way it is declared.</strong> What you type is
  matched three ways, best first, and which one matched is part of the ordering — a name that
  starts with what you typed is offered above one that merely spells its humps that way:
</p>
<ol>
  <li><strong>An exact prefix</strong> — <code>toLo</code> → <code>toLowerCase</code>.</li>
  <li><strong>A prefix ignoring case</strong> — <code>TOLO</code>, or a shift key held one letter
    too long.</li>
  <li><strong>The camel humps</strong> — <code>tolc</code> → <code>toLowerCase</code>,
    <code>aAE</code> → <code>addAllElements</code>, <code>SBA</code> →
    <code>SpringBootApplication</code>. This is how anyone who already knows a name reaches for it.
    An underscore starts a word too, so <code>MAXV</code> reaches <code>MAX_VALUE</code>.</li>
</ol>
<p>
  It is <strong>one rule for every kind of candidate</strong>: a member, a local, a static import, a
  class name. That is worth saying because it used to be two — the humps existed for class names
  only, and everything else was matched by a literal, case-sensitive prefix.
</p>
<p>
  <strong>Settings → Completion → Popup</strong> decides when the popup arrives and how forgiving
  it is: whether it opens on its own while you type (and after how long a pause), or only on the
  chord; and <em>case-sensitive matching</em>. That last one is about <em>case</em> and not about
  the humps: off, only the first letter has to agree — which in Java is the letter that carries
  information, since a capital means a type; on, every typed letter does, so <code>aAE</code> still
  reaches <code>addAllElements</code> and <code>aae</code> no longer does.
</p>

<h2>A name with nothing to its left</h2>
<p>
  A bare identifier is not a mystery: Java says exactly what it can mean, and the popup offers
  those things in order of how near they are to the caret.
</p>
<ul>
  <li><strong>What the scope binds</strong> — locals, method and lambda parameters (typed or not),
    the <code>for</code> variable, a try-with-resources, a <code>catch</code> parameter, a pattern
    variable from <code>o instanceof Foo f</code>. The nearest wins: a variable declared in this
    block beats a parameter of the method around it, and among names declared in the same block,
    the one on the line above beats the one twenty lines up. A name that shadows another is offered
    once, and it is the inner one — the same rule that decides what the name would resolve to.</li>
  <li><strong>The enclosing type's own members</strong>, and everything it inherits, written without
    <code>this.</code>.</li>
  <li><strong>Whatever an <code>import static</code> brought in</strong>, with the type it came from
    on the row — the one case where nothing else on screen says where a bare name is from.</li>
  <li><strong>Class names</strong>, from the type-name index described below.</li>
</ul>
<p>
  A local is not in scope inside its own declaration, so <code>int counted = coun</code> offers
  <code>counter</code> and not <code>counted</code>. And in a <code>static</code> method an
  instance member is not offered at all: <code>count</code> inside <code>static void main</code>
  does not compile, however visible the field is from elsewhere.
</p>
<p>
  Whether the class names come before or after the rest is decided by Java's own naming convention
  rather than by a score, because the two lists are not comparable — <code>Str</code> is a type
  being written and <code>str</code> is a variable, and no amount of ranking makes one the other.
  A capitalised name with a lowercase letter in it leads with types; anything else leads with what
  is in scope, which is where a <code>SCREAMING_CASE</code> constant of your own class lives. Both
  lists are always offered, so a prefix read the "wrong" way costs a scroll, not an answer.
</p>

<h2>What accepting one writes</h2>
<p>
  A method is a call, so accepting one writes <code>size()</code> and not <code>size</code>. When
  it takes something, the caret lands <strong>between the parentheses</strong> — which is also
  where the parameter hints strip is about to describe what goes there. When it takes nothing, the
  caret lands after them and the call is finished.
</p>
<p>
  The parameter <em>names</em> are not written. Filling in <code>put(key, value)</code> as
  placeholders to be tabbed through and deleted is slower than typing the arguments, and a class
  file carries no parameter names to fill in with anyway.
</p>
<p>
  When the call is <strong>already written</strong> — <code>t.si()</code> with the caret after
  <code>si</code>, which is how you correct the name of an existing call — the parentheses are left
  alone rather than doubled.
</p>

<h2>Abbreviations</h2>
<p>
  Type <code>psf</code> and the list offers <strong>public static final</strong>, with
  <code>psfi</code> and <code>psfs</code> under it for the <code>int</code> and the
  <code>String</code>. These are IntelliJ's own Java abbreviations, deliberately: somebody who has
  typed <code>psf</code> for fifteen years should get it here too, without configuring anything.
</p>
<ul>
  <li><strong>Modifiers</strong> — <code>psf</code>, <code>psfi</code>, <code>psfs</code>,
    <code>prsf</code>, <code>prsfi</code>, <code>prsfs</code>, and <code>psvm</code> for a
    <code>main</code>.</li>
  <li><strong>Printing</strong> — <code>sout</code>, <code>souf</code>, <code>serr</code>.</li>
  <li><strong>Statements</strong> — <code>fori</code>, <code>ifn</code>, <code>inn</code>,
    <code>thr</code>.</li>
</ul>
<p>
  The ones with more than one blank tab through with <kbd>Tab</kbd>, and the body is
  <strong>re-indented</strong> to where it lands, so a <code>psvm</code> three levels into a class
  arrives lined up rather than at the margin.
</p>
<p>
  They are <em>added</em> to what the index found, never substituted: a field actually called
  <code>psfCount</code> still appears, under them. And they are offered only where a bare word is
  being typed — after a <code>.</code> the list is answering "what members does this have", and an
  abbreviation there would be a wrong answer to a precise question.
</p>
<p>
  What is deliberately missing is the abbreviations that have to <em>read the surrounding code</em>
  to be right — IntelliJ's <code>iter</code>, which infers the collection in scope. A version that
  guesses writes a name that is not there, and an abbreviation is supposed to save typing rather
  than start a correction.
</p>

<h2>Members that do not exist yet</h2>
<p>
  Two families, and they are the same gesture: you are in a class body, you start typing a name,
  and the thing you mean has not been written.
</p>

<h3>The method this class calls and does not declare</h3>
<p>
  You wrote <code>randomico()</code> inside a method; now <code>rand</code> in the class body offers
  to declare it. The <strong>call site is the specification</strong>: the arguments' declared types
  and names become the signature, what the result is used as becomes the return type, and a call
  from a <code>static</code> method asks for a <code>static</code> one. It is the same reading the
  <em>Create method</em> quick fix makes, so the popup and Alt+Enter cannot describe one member two
  ways.
</p>
<p>
  Read from the <strong>tree</strong>, not from the diagnostic — which is what makes it survive
  being typed. A half-written name in a class body is a syntax error, and the member it is in stops
  being validated while it stays one, so the red mark under <code>randomico()</code> disappears at
  exactly the keystroke where the offer is wanted. The call is still in the tree, though, beside
  the ERROR node the half-written name recovered as.
</p>
<p>
  It also reads the calls made on this class from <strong>elsewhere in the file</strong> — which is
  what a <strong>nested</strong> class needs: <code>c.randomico("ciao")</code> is written in the
  outer class, on an instance of the inner one, so the call that describes the method lives outside
  the inner class entirely. Standing in the inner class, <code>ra</code> offers it, as a
  <code>public</code> method since another class is calling it. Only a receiver whose type actually
  resolves to the class you are standing in counts.
</p>
<p>
  A method the class <strong>inherits</strong> is not missing, and offering to write one is offering
  to break the build — so the supertypes are walked first, with the same conservatism the
  unknown-member check applies: an unreadable hierarchy means silence.
</p>

<h3>The accessors a field is missing</h3>
<p>
  Typing <code>getCust</code> in a class body offers <strong><code>getCustomer()</code></strong>,
  and accepting it writes the whole method. Three accessors per field:
</p>
<ul>
  <li><strong><code>getX</code></strong> — or <code>isX</code> for a primitive
    <code>boolean</code>, which is what JavaBeans says and what every framework that reflects over
    accessors looks for.</li>
  <li><strong><code>setX</code></strong>, with <code>this.</code> on the assignment: the parameter
    shadows the field, and without it the assignment is the parameter to itself.</li>
  <li><strong><code>withX</code></strong> — the builder-style setter, which assigns and returns the
    object so calls chain. The same member <strong>Generate</strong> writes under "With".</li>
</ul>
<p>
  A <code>final</code> field is assigned once, at construction: it is offered a getter and nothing
  else. A <code>static</code> one is offered no <code>withX</code> — there is no <code>this</code>
  to return. A field named <code>isActive</code> keeps the prefix it has and is set and chained by
  its <em>property</em>: <code>setActive</code>, <code>withActive</code>.
</p>
<p>
  It is not a guess. A field with no getter is a fact about the text, the accessor's name is Java's
  own convention, and its body is the only body it could have. When exactly one candidate matches,
  the same answer is also available as <strong>ghost text</strong> ahead of the caret —
  <code>→ public String getCustomer() &lbrace; … &rbrace;</code>, written with <kbd>Tab</kbd>. Two
  candidates produce nothing, however close the second is: ghost text sits where it reads like text
  that is already there, so being wrong there costs trust rather than a keystroke.
</p>
<p>
  <strong>While the popup is open, the grey text previews the highlighted row.</strong> A row
  labelled <code>getCustomer</code> whose insertion is four lines of code tells you almost nothing
  about what <kbd>Enter</kbd> will do, so what that row would write is drawn at the caret — and it
  follows the arrow keys. It is the popup's own selection rendered, not a second proposal:
  <kbd>Tab</kbd> still belongs to the popup, and accepting the preview is refused outright, so the
  two can never both insert.
</p>
<p>
  There it is <strong>one line</strong> — <code>public String getCustomer() &lbrace; … &rbrace;</code>
  — because the popup opens directly under the caret, and a four-line preview drawn there is a
  four-line preview with a list on top of it. The signature is the half that carries the
  information; the body of a generated member is implied by its name. The whole text is still what
  accepting writes, and it is what the panel beside the list shows.
</p>
<p>
  With the popup closed — after <kbd>Esc</kbd>, or with the auto-popup off in
  <strong>Settings → Completion</strong> — the proposal stands on its own and is drawn in full,
  because nothing is competing for the space. <kbd>Tab</kbd> writes it.
</p>
<p>
  Three things keep it quiet where it is not wanted: it is offered <strong>only at a member
  position</strong> — not inside a method body, where <code>getCustomer</code> is a call, and not
  after a dot; only for a field that <strong>has no accessor already</strong>; and never on an
  empty prefix, which would open a three-field class on six generated members ahead of everything
  real. A <code>final</code> field is offered no setter.
</p>
<p>
  <strong>Generate</strong> (<kbd>Alt</kbd> + <kbd>Insert</kbd>) is still the place for the whole
  set at once, and it is the one that carries the style options — fluent accessors, snake_case
  naming, <code>final</code> parameters. A completion candidate has no conversation, so it takes
  the conventional form: <code>public</code>, the field's own type, <code>this.</code> on the
  assignment.
</p>

<h2>Reading a row</h2>
<p>
  Each row is four columns rather than a run-on line: what it <strong>is</strong> (the kind icon),
  what it is <strong>called</strong> (with the letters you typed marked), its
  <strong>shape</strong> — <code>(String, int) : void</code> — and, pushed to the right edge,
  where it <strong>comes from</strong>. That last one is what tells <code>List.of</code> from
  <code>Set.of</code>, and a method you inherited from one your own class declares. A deprecated
  candidate is struck through; it is still offered, since it exists and you may be reading old
  code.
</p>
<p>
  The <strong>documentation</strong> of the highlighted row appears beside the list — the same card
  the hover tooltip draws, from the same answer, so the two cannot describe one member two ways.
  It is fetched for the row you actually highlight and not for the four hundred in the list: a
  library's documentation is read out of its sources archive on disk, and resolving all of them
  would put an archive read per candidate on the keystroke that opened the popup.
</p>

<h2>Completions</h2>
<p>
  Typing <code>.</code> after an expression offers member completions; press
  <kbd>Ctrl</kbd> + <kbd>Space</kbd> (or <kbd>Ctrl</kbd> + <kbd>Shift</kbd> + <kbd>Space</kbd>) to
  request them explicitly, anywhere — on macOS <kbd>Cmd</kbd> + <kbd>Shift</kbd> +
  <kbd>Space</kbd>. In Java, completions
  come from the project index and appear once it is warm. Edits re-index in the background as you
  type, so completion and go-to-definition track your changes without reopening the project. In
  <code>.dig</code> and <code>.dev</code> they come from <strong>nd-dig-lsp</strong>, geode's own
  language server — see <em>geode <code>.dig</code> scripts</em> above.
</p>
<p>
  <strong>Command languages ask on every position.</strong> A <code>.dev</code> scenario or a
  <code>.dig</code> line is words and arguments rather than dotted paths, and its vocabulary lives
  right after a space: with the caret at the end of <code>unlock&nbsp;</code> the server already
  knows what may follow, so it is asked there too — not only after a trigger character.
</p>
<p>
  When an explicit request finds nothing, the editor footer says so for a moment
  (<em>No suggestions here</em>, or <em>No engine answered for this file</em> when no server or
  index is up). Silence would mean the same thing as a shortcut that never arrived, and those two
  have opposite fixes.
</p>
<p>
  <strong>On macOS the chord is <kbd>Cmd</kbd> + <kbd>Shift</kbd> + <kbd>Space</kbd></strong>, and
  that is not a style choice. macOS claims the whole <kbd>Ctrl</kbd> + <kbd>Space</kbd> family for
  switching input source, and it claims it <em>above</em> applications: those two chords produce no
  key event at all in any program, so there is nothing an editor could bind them to.
  <kbd>Cmd</kbd> + <kbd>Space</kbd> is left alone as well — it is Spotlight — which is what the
  <kbd>Shift</kbd> is for.
</p>
<p>
  If a shortcut seems to do nothing at all, turn on <strong>Show keyboard inputs</strong>
  (<kbd>Alt</kbd> + <kbd>Shift</kbd> + <kbd>K</kbd>, or the Command Palette) and press it again:
  the overlay draws every chord the window receives. Nothing drawn means the key never arrived,
  which is a keyboard conflict outside Bennu and not something Bennu can answer.
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
  Within each tier of the match described under <em>How a name is matched</em>, a type
  <em>your project</em> declares comes before one out of a jar, and the shorter name before the
  longer.
</p>
<p>
  <strong>A qualified name completes one segment at a time</strong> — in an <code>import</code>, and
  anywhere a name is written out in full. Typing <code>import org.</code> offers
  <code>springframework</code>, not everything beneath it; <code>import
  org.springframework.boot.</code> offers the packages under it beside the classes in it, and a
  class row shows the full name it lands on. An <code>import</code> is the one line in a Java file
  written entirely in fully-qualified names, and it is the one an editor can help with most.
</p>
<p>
  A receiver whose type is <strong>not imported yet</strong> still completes: typing
  <code>Arrays.</code> in a file with no <code>import java.util.Arrays;</code> offers that class's
  members, and accepting one adds the import along with it. Only an unambiguous name is taken — a
  simple name several packages declare is left alone rather than guessed at.
</p>
<p>
  A type receiver also offers its <strong>nested types</strong>: <code>Outer.</code> lists
  <code>Inner</code> beside the statics and constants, since that is how a nested type is named.
  This holds for a library type as well — <code>Map.</code> offers <code>Entry</code>.
</p>
<p>
  <strong>A nested type completed by its simple name is written through its outer.</strong>
  <code>Inner</code> alone is not a name Java resolves — a nested type is reached through the class
  that declares it, or through an import that names it exactly — so accepting <code>MyProva</code>
  writes <code>ConfigurazioneCors.MyProva</code> and imports <code>ConfigurazioneCors</code>. One
  import serves every nested type of that class, and it is the form people write by hand. Where the
  simple name is already in scope nothing is added: inside the outer class itself (or a sibling
  nested in it), and in a file that imports the nested type outright.
</p>
<p>
  In <strong>TypeScript</strong> and <strong>JavaScript</strong> — and in a <code>.svelte</code>
  file, and in an Angular project's templates — the popup is the language server's:
  <code>typescript-language-server</code> for the JS family, <code>svelteserver</code>,
  <code>ngserver</code>. Install it from Settings → Language Servers; without one the file is still
  coloured, folded and edited, it simply has nothing to suggest.
</p>
<p>
  A JSP's <code>&lt;script&gt;</code> body is the exception, and deliberately: no language server
  will ever serve a JSP — it is a template that <em>prints</em> JavaScript, half of it
  <code>&lt;%= %&gt;</code> holes — so Bennu answers it itself. The names declared in the block
  (<code>var</code>, <code>function</code>, <code>foo: function (…)</code> and their parameters)
  are offered first, then the browser globals. After <code>document.</code>,
  <code>location.</code>, <code>Math.</code> and the rest of the platform, the members offered are
  <strong>read off the object itself</strong> in the engine the page will run in — not a list typed
  into Bennu, so it cannot be stale. Hover says the same: a local shows the line it was declared
  on, a platform function how many arguments it really takes. What it cannot know is jQuery:
  <code>$</code> is offered as a name, but <code>$(…).</code> has no object to read.
</p>
<p>
  An <strong>overloaded</strong> method is offered as <em>one row</em>, whose shape says
  <em>+2 overloads</em>. Accepting a completion writes the method's name and its parentheses, not
  its arguments, so three rows would be three chances to choose with one outcome — and they would
  push the members you were looking for off the popup. Nothing is hidden by the fold: the
  <strong>parameter hints</strong> strip shows the whole set the moment you type inside the
  parentheses, which is when knowing them starts to matter and when they can be shown properly,
  one at a time, with the argument you are on marked. A method that merely
  <strong>overrides</strong> an inherited one appears once. Inherited members are included; a
  <code>private</code> member of another class is not.
</p>
<h2>Where the import counts come from</h2>
<p>
  They are counted during the <strong>index build</strong>, in the pass that already parses every
  file — one hash lookup per <code>import</code> line, on a walk that was happening anyway. Nothing
  is written to disk and nothing is asked of you: it is a fact about the project's own sources,
  and a saved copy of it would be a copy that goes stale, which is worse than none. A project that
  dropped a library would go on recommending it and nothing would say why.
</p>
<p>
  It is <strong>not</strong> updated as you type, and that is deliberate too. Whether this is a
  codebase that uses <code>java.util.List</code> is not a fact that changes because you saved a
  file, and a completion order that visibly shifted under you would be worse than one a rebuild out
  of date. It refreshes whenever the index does.
</p>
<p>
  A count is used as a <strong>band</strong>, not as a number: each step is a doubling, and it stops
  at 128. The difference between four imports and eight is real; the difference between four hundred
  and eight hundred is not, and a ranking that pretended otherwise would let one ubiquitous type sit
  at the top of every list it matches for ever. A wildcard import counts for the whole package, one
  band weaker — <code>import java.util.*</code> is a file choosing the package, not the type.
</p>

<h2>Annotations</h2>
<p>
  Typing <strong><code>@</code></strong> opens the popup on its own, and it contains
  <strong>annotation types only</strong>. That character narrows the legal names harder than
  anything else in Java — from every type on the classpath to the few hundred annotations on it —
  so the list at that moment is short and nearly always holds the answer.
</p>
<p>
  It narrows again from where the caret is. An annotation declares what it may be attached to
  (<code>@Target</code>), so above a field the ones that only go on a method sort below the ones
  that belong there. They are ranked rather than hidden: an annotation from a jar whose bytes have
  not been read reports no target at all, and reading that as a refusal would hide it entirely.
</p>
<p>
  The list is complete from the <strong>first letter</strong>, and that is worth saying because the
  sweep it comes from is bounded: what is capped is how many annotations are offered, not how many
  names are looked at on the way to them. <code>@S</code> reaches
  <code>SuppressWarnings</code> past the several hundred ordinary classes that also begin with
  <code>S</code>.
</p>
<p>
  Accepting one adds its import, on the same terms as any other type name — and the ones you
  accept are offered ahead of the ones you don't, for as long as the project stays open. That is
  the same short-term memory member completion has, keyed here to the <code>@</code> rather than
  to a declaring type; it is never written to disk, and nothing about it outranks an import the
  file has already made.
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
  <strong><code>@Builder</code></strong> is modelled as the class it really generates: the static
  <code>builder()</code>, a setter per field returning the builder so the chain stays typed,
  <code>build()</code>, and — with <code>toBuilder = true</code> — the instance
  <code>toBuilder()</code> that starts from an existing object. <code>@Singular</code> adds the two
  methods it is written for: the single-element adder (<code>.tag("a")</code>, named from the field
  or from <code>@Singular("tag")</code>) and <code>clearTags()</code>. A field marked
  <code>@Builder.Default</code> is still a constructor parameter — Lombok moves its initializer out
  of the field, so the constructor does assign it.
</p>
<p>
  <code>@Builder</code> is also read where Lombok allows it to be written: on a
  <strong>constructor</strong> or a <strong>static factory</strong>, where the builder takes that
  element's <em>parameters</em> rather than the class's fields, is named after what it builds, and
  <code>build()</code> returns that.
</p>
<p>
  Two shapes generate members that depend on a type this cannot read while indexing:
  <code>@Delegate</code>, which copies every public method of a field's type onto the owner, and a
  <code>@SuperBuilder</code> builder, which carries its parent's setters. Both mark the type as
  having <strong>more members than the list</strong>, so nothing concludes "no such method" from
  their absence — the rest of the type is still checked exactly as before.
</p>
<p>
  <code>@Value</code> is honoured as what it stands for: the fields are <code>private final</code>
  and the class is <code>final</code>, though the source writes none of that.
  <code>@FieldDefaults(makeFinal = true, level = …)</code> does the same, and the per-field escape
  hatches — <code>@NonFinal</code>, <code>@PackagePrivate</code> — are honoured.
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

<h2>Postfix templates</h2>
<p>
  Write the expression first, then say what to do with it. Type a dot after any expression and a
  short word — <code>order.getTotal().nn</code> becomes
  <code>if (order.getTotal() != null) &lbrace; … &rbrace;</code>, with the caret in the body. The
  point isn't the keystrokes saved: it lets you think in the order you actually think, the value and
  then the control flow around it, instead of committing to an <code>if (</code> before you know what
  goes in it.
</p>
<p>
  They appear in the completion list under the name you type, so they're discovered the way
  everything else is. The names are IntelliJ's on purpose — muscle memory is the whole value of a
  postfix template.
</p>
<ul>
  <li><strong>Null checks</strong> — <code>.nn</code>, <code>.null</code></li>
  <li><strong>Control flow</strong> — <code>.if</code>, <code>.else</code>, <code>.while</code>,
    <code>.for</code>, <code>.fori</code>, <code>.forr</code>, <code>.switch</code>,
    <code>.try</code>, <code>.synchronized</code></li>
  <li><strong>Statements</strong> — <code>.var</code>, <code>.return</code>, <code>.throw</code>,
    <code>.assert</code>, <code>.sout</code>, <code>.serr</code></li>
  <li><strong>Expressions</strong> — <code>.not</code>, <code>.par</code>, <code>.cast</code>,
    <code>.instanceof</code>, <code>.opt</code>, <code>.stream</code>, <code>.forEach</code></li>
</ul>
<p>
  What is offered follows the project's <strong>Java level</strong>: on a project below Java 10,
  <code>.var</code> and <code>.for</code> leave you a stop where the type goes instead of writing
  <code>var</code>, which wouldn't compile there. Rust gets its own postfix set from rust-analyzer,
  in the same list.
</p>
