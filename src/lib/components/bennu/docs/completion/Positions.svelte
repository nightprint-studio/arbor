<script lang="ts">
  /** Completion — positions with their own rules. */
</script>

<span class="eyebrow">Completion</span>
<h1>Positions with their own rules</h1>

<p class="doc-lead">
  After a dot, in arguments, in annotation values, in a declaration: what each position offers.
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
