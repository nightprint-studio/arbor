<script lang="ts">
  /** Completion — the order things are offered in. */
</script>

<span class="eyebrow">Completion</span>
<h1>The order things are offered in</h1>

<p class="doc-lead">
  What comes first, and why: eight signals, strongest first.
</p>

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
<p>
  At the start of a call argument the list is then reordered once more by a small model fitted offline on public code. It reads where the
  list put each row, what kind of row it is, how recently this file used the name, how much the name echoes the parameter's own, and whether its
  type is the parameter's. It only reorders: nothing is hidden or added, a habit that leads the list keeps its place, and it follows the same
  switch as the learned picks. Nothing is learned on your machine by it.
</p>
