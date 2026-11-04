<script lang="ts">
  /** Completion — smart suggestions and ghost text. */
  import Callout from '$lib/components/shared/ui/Callout.svelte';
</script>

<span class="eyebrow">Completion</span>
<h1>Smart suggestions and ghost text</h1>

<p class="doc-lead">
  The learned and project-aware signals, the rest of the line drawn in grey, and the type names that bring their import.
</p>

<h2>Smart suggestions</h2>
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

<h2>Finishing the line</h2>
<p>
  Beside the popup, Bennu can draw the <strong>rest of the line</strong> in grey after the caret: <kbd>Tab</kbd> writes it, <kbd>Esc</kbd> dismisses it.
  It has several sources, and all but the last need no model.
</p>
<dl class="meta-grid">
  <dt>A constructor</dt>
  <dd>Type <code>public Order(</code> in a class with fields and the whole constructor is drawn: the parameters, the closing parenthesis and a body that assigns each
    one. It takes the instance fields that have no value — not <code>static</code> ones, not a <code>final</code> field that already has one — and it is not offered when the class
    already has a constructor with that many parameters or when the name is not the class's own. It is written from the class, not guessed, so it needs no model.
    A block of several lines can be taken <strong>one line at a time</strong>: <kbd>Ctrl</kbd> + <kbd>→</kbd> (<kbd>Cmd</kbd> + <kbd>→</kbd> on macOS) writes the
    first line and keeps the rest drawn; <kbd>Tab</kbd> writes what is left. The closing parenthesis the editor added with the <code>(</code> is overwritten.</dd>
  <dt><code>equals</code>, <code>hashCode</code>, <code>toString</code></dt>
  <dd>Type <code>public boolean equals(</code>, <code>public int hashCode(</code> or <code>public String toString(</code> and the method is drawn from the instance fields: <code>==</code> for
    primitives, <code>Double.compare</code> and <code>Float.compare</code> for floating point, <code>Arrays.equals</code> for arrays and <code>Objects.equals</code> for the rest, with
    <code>transient</code> fields left out of the first two. A class that declares the method already gets nothing, and so does one without fields. <code>Objects</code> and
    <code>Arrays</code> are written short when the file imports them, and as <code>java.util.Objects</code> when it does not, so the block compiles as it is. A long expression breaks between
    fields, before an operator.</dd>
  <dt>Methods the class owes its supertypes</dt>
  <dd>Start the name of a method the class has to implement or may override — <code>compa</code>, <code>int compareTo(</code> — and the whole declaration is drawn, <code>@Override</code>
    above and a body that compiles (<code>return 0;</code>, <code>null</code>, nothing for <code>void</code>). The list is the one behind <em>Implement / override methods</em>; abstract
    methods come first, a method that is only overridable needs its full name, and one the class already declares is not offered again. A method whose types the file would need an
    import for is left to that dialog.</dd>
  <dt>A test for a method</dt>
  <dd>In <code>OrderTest</code>, start <code>total</code>, <code>testTotal</code> or <code>void total</code> and a <code>@Test</code> method is drawn that builds an <code>Order</code> and calls
    <code>total()</code> with the default of each parameter's type. The class under test is the one the file name points to under <code>src/main/java</code>; the assertions are yours. Private
    methods and tests already written are not offered.</dd>
  <dt>Lines copied from the project</dt>
  <dd>Every line the project's files hold is counted at each word start, as the last few words before it and what followed them. When you reach the same words, the rest of the line most often written after them is drawn, with the share of the sightings as its confidence. It needs the words seen at least twice, and below half of the sightings nothing is drawn. A strong share (70% or more) goes before the model; a weak one is only used when the model has no answer. When no ending is sure but most sightings start alike, only the words they agree on are drawn (<code>findBy</code> and not the arguments), at least two of them and with at least 70% agreeing. Lines under 25 characters, imports and comments are not counted. <em>Forget what Bennu learned</em> empties it. <strong>Copy lines from the project</strong> in Settings › Completion › Ghost text switches it off.</dd>
  <dt>Small layers you can switch off</dt>
  <dd>Four layers have a switch each in Settings › Completion › Ghost text: <strong>Suggest variable names</strong> (on: after a type, <code>Order ▌</code> becomes <code>order</code>), <strong>Suggest usual arguments</strong> (on), <strong>Suggest creation arguments</strong> and <strong>Repeat list items</strong>
    (both off by default: measured on public projects, fewer than one suggestion in ten was useful to the person typing). Switch one to change it.</dd>
  <dt>Getter and setter bodies</dt>
  <dd>On the empty line under <code>public String getName() &#123;</code> the body is drawn from the field of the same name and type — <code>return name;</code> — and under <code>public void setName(String name) &#123;</code> it is <code>this.name = name;</code> (and <code>return this;</code> for a setter that returns the class). The style, <code>this.x</code> or <code>x</code>, is the one the file's other accessors use. Without such a field, or with a <code>static</code> one, a field of another type or an array, nothing is drawn.</dd>
  <dt>The assignment a constructor is missing</dt>
  <dd>Add a field and a parameter of the same name to a constructor and, on an empty line in its body, <code>this.field = field;</code> is drawn for the first field the body does not set yet. A field with a value, a <code>static</code> one and a parameter with no field by its name are left alone.</dd>
  <dt>The function a call takes</dt>
  <dd>At <code>users.stream().map(</code> the function the project usually passes to that call is drawn — <code>User::getName</code>, or <code>u -&gt; u.getName()</code> if that is how it writes it — for the type the index knows the function receives. Both must agree: the method has to exist on that type, and the project has to have passed it to that call at least twice and in at least 40% of the cases among that type's methods.</dd>
  <dt>What a test checks</dt>
  <dd>In a test file, on the empty line under a local variable, the assertion the project's tests write under such a variable is drawn up to the value you have to supply: <code>assertThat(total).isEqualTo(</code>. It needs the same shape at least three times and at least half of the assertions that follow a declaration.</dd>
  <dt>Handling an exception</dt>
  <dd>On the empty line after <code>catch (SQLException e) &#123;</code> the statement this project usually writes first for that exception is drawn, with the variable's name taken from the block in front of you:
    <code>throw new DataAccessException("failed", e);</code>. It needs the same handling at least three times and at least 60% of the catches of that type. An empty <code>catch</code> teaches nothing, and a multi-catch has no one type, so neither is counted.</dd>
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
  <dt>The next statement of a method</dt>
  <dd>On the empty line inside a method that already has a statement, the next one is drawn from the methods of the open project that begin the same way and have a similar signature — the same shape of statement, names and values aside —
    with their names changed to yours: parameters by position, locals by order of declaration, the other names chosen among your parameters, locals and fields. It is drawn only when at least seven in ten of those methods agree on the shape of what comes next
    (from the third statement on, the shape most often seen after the last three decides), and not when it would read a variable you do not have. It learns only from the project that is open, is held in memory and is never saved.</dd>
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
  <dt>One name, any case</dt>
  <dd>The type a project creates under a variable name and the statement that follows another are counted by the <em>words</em> of the name, so <code>jaxbProcessor</code>,
    <code>jaxb_processor</code>, <code>JaxbProcessor</code> and <code>JAXB_PROCESSOR</code> are one name: what is learned in a file written one way helps in another written the other way.
    The page of what Bennu has learned lists such a name once, as its words joined by underscores.</dd>
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
  adapted to your own code), how sure the model has to be before it is drawn, a switch for the log statements, <strong>Count how often suggestions are taken</strong> (only how many suggestions of each kind were shown, taken with Tab and dismissed, per confidence level, in your profile; no code is stored and nothing is sent: when the model's suggestions are taken at least half the time above some confidence, the page offers that value and changes it only when you press the button), <em>What Bennu has learned…</em> (command palette, or the <strong>Open…</strong> button in Settings › Completion › Learning) lists what the completion counted and lets you stop offering a row or forget a counter of your own. Where a parameter is a <code>Class&lt;T&gt;</code> the list opens on the usual classes already written as <code>String.class</code>, and on the project's implementors where <code>T</code> is bounded; after a comma, the remaining arguments are drawn by their types. Where a call always takes a call on some variable (<code>new ByteArrayInputStream(bao.toByteArray())</code>), the ghost text draws it with the nearest variable in scope that has that method. On an empty line, after a statement the project always follows the same way, the start of the usual next call is drawn (<code>jaxb_processor.marshal(</code>); only the start, since the arguments are filled by the other layers. <strong>Remember what Bennu learned in each project</strong> also lets suggestions follow how the project is laid out: the space after <code>if</code>/<code>for</code>, before a brace and after a comma are measured from its files and used where the project is consistent, never inside string literals. <strong>Ignore commented-out code</strong> (off by default: blocks of code in <code>//</code> lines are left out of what the model reads, so it does not copy a version you commented out; explanations in comments are still read), Where a project creates the same type under a name nearly every time (<code>bao</code> is always a <code>ByteArrayOutputStream</code>), writing <code>val bao</code> draws the rest of the statement, and the popup after <code>new</code> lists that type first; it is listed under <em>Types by variable name</em> in <em>What Bennu has learned…</em>. <strong>Learn from corrections</strong> (off by default: after you take a suggestion it notes whether you left it, cut its tail, wrote on, changed a part, only re-lay it out — spaces, line breaks, indentation, which is not a change of content — or took it back, once you have moved on; a part you change and put back as it was counts as left alone; counters per kind of suggestion, in your profile, nothing sent; a kind you rarely keep is drawn with up to a third less confidence, so it shows less often — a suggestion you take and then remove whole is not counted against it, since going another way is not the suggestion being wrong, while one you never take is; <em>Keep the text too</em> adds a log of the texts capped at 1 MB; <em>Notify when it learns</em> shows each counted correction as a notification, with the kind of suggestion, where, what happened and how many of that kind were seen — for checking what is learned; <em>Forget</em> deletes both), <strong>Check with Bennu</strong> (the model's line is run through the code checks of the method you are in and dropped when it calls a method the receiver does not have, passes the wrong arguments or uses a variable that does not exist; a class you have not imported yet does not count against it, nor does a statement that is not finished), and <strong>Try three candidates</strong>: the model
  gives three answers and the first that passes the checks is drawn, instead of nothing when its only answer fails. It is several times slower (about 280 ms against 60 in the lab), so it is off by default. The palette has
  <em>Model suggestions: turn on / off</em>.
</p>

<h2>Type names</h2>
<p>A simple name usually resolves to several types, so type names get two terms of their own:</p>
<dl class="meta-grid">
  <dt>What this file has said</dt>
  <dd>Nothing outranks it: a type the file imports, then a package it wildcard-imports, then its own package, which needs no import.</dd>
  <dt>Accepting a type name imports it</dt>
  <dd>
    With <em>Auto-import on accept</em> on, the import is added when you accept a type name from the list — after <code>new</code>, in a
    declaration, a generic, a cast, an annotation, a <code>catch</code>, a <code>throws</code> — and a single <kbd>Ctrl</kbd> + <kbd>Z</kbd>
    takes back the name and the import together. A name several packages share is imported when the ranking above has a clear winner and the
    file reaches none of them yet; on a tie nothing is imported and <kbd>Alt</kbd> + <kbd>Enter</kbd> asks which.
  </dd>
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

<h2>Where the import counts come from</h2>
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
