<script lang="ts">
  /** Completion — members that do not exist yet. */
  import Callout from '$lib/components/shared/ui/Callout.svelte';
  import { highlightCode } from '$lib/utils/highlight';
</script>

<span class="eyebrow">Completion</span>
<h1>Members that do not exist yet</h1>

<p class="doc-lead">
  Members written nowhere in the source: the ones you are about to create, and the ones the compiler or Lombok generate.
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
