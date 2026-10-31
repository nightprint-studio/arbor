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

<h2>Type names</h2>
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
