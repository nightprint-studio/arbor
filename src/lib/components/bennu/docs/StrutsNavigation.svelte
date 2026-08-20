<!-- Bennu docs — Struts: navigating between actions, configuration and views. -->
<h1>Struts navigation</h1>
<p class="doc-lead">
  In a Struts application the behaviour is spread across XML rather than expressed in the code.
  These are the jumps that put it back together: an <code>action="…"</code> string on one side,
  a configuration entry, a class and a view on the other.
</p>

<h3>Struts navigation</h3>
<p>
  <kbd>Ctrl</kbd> + <kbd>B</kbd> (or <kbd>Ctrl</kbd> + click) on an
  <code>action="…"</code> reference jumps to the Struts <code>&lt;action&gt;</code> config, its view
  JSP, or the action class. <kbd>Alt</kbd> + <kbd>F7</kbd> on an action reference lists every JSP
  that uses it. The same keys work on a <strong>page-scoped JSP variable</strong> — a
  <code>&lt;c:set var="x"&gt;</code>, <code>&lt;s:set var="x"&gt;</code>,
  <code>&lt;c:forEach var="x"&gt;</code> or <code>&lt;s:iterator var="x"&gt;</code> and its
  <code>$&lbrace;x&rbrace;</code> / <code>%&lbrace;x&rbrace;</code> references: <kbd>Ctrl</kbd> +
  <kbd>B</kbd> on a reference jumps to where the variable is set, and <kbd>Alt</kbd> +
  <kbd>F7</kbd> lists every reference in the page (it's page-scoped, so all in the same file).
  <kbd>Ctrl</kbd> + <kbd>B</kbd> (or <kbd>Ctrl</kbd> + click) on a JSP <strong>include path</strong> —
  a <code>&lt;%@ include file="…"&gt;</code> directive, <code>&lt;jsp:include page="…"&gt;</code> or
  <code>&lt;s:include value="…"&gt;</code> — opens the referenced JSP.
  An absolute action reference that resolves to nothing gets a
  <strong>warning squiggle</strong> — a wildcard or runtime (<code>$&lbrace;…&rbrace;</code>/<code>%&lbrace;…&rbrace;</code>)
  reference never does. A static <strong>include</strong> whose target file doesn't exist
  (<code>&lt;%@ include file="…"&gt;</code>, <code>&lt;jsp:include page="…"&gt;</code>,
  <code>&lt;s:include value="…"&gt;</code>, <code>&lt;c:import url="…"&gt;</code>) is flagged the
  same way — a computed or <code>http(s)://</code> reference never is. Inline
  <code>&lt;script&gt;</code> and <code>&lt;style&gt;</code> blocks are highlighted as JavaScript and CSS.
</p>
<p>
  While editing a JSP, the editor toolbar shows an <strong>Insert tag</strong> menu that drops a
  ready-made JSTL / Struts snippet at the caret — <code>&lt;c:set&gt;</code>, <code>&lt;s:set&gt;</code>,
  <code>&lt;s:property&gt;</code>, <code>&lt;s:iterator&gt;</code>, <code>&lt;c:forEach&gt;</code>,
  <code>&lt;s:if&gt;</code> / <code>&lt;c:if&gt;</code>, <code>&lt;s:url&gt;</code>, <code>&lt;s:text&gt;</code> and
  <code>&lt;s:textfield&gt;</code> — with placeholder attributes you overtype.
</p>
<p>
  A <strong>form field</strong> in a JSP — the <code>name="…"</code> of an <code>&lt;s:textfield&gt;</code>,
  <code>&lt;input&gt;</code>, <code>&lt;s:select&gt;</code>, … inside a form — is understood as a property of
  the form's <strong>action class</strong>. <kbd>Ctrl</kbd> + <kbd>B</kbd> (or <kbd>Ctrl</kbd> + click)
  on it jumps to the matching <code>get</code>/<code>set</code>/<code>is</code> accessor in the action
  Java; and a field whose name is <strong>not</strong> a property of the action gets a
  <strong>warning squiggle</strong> (a likely typo — “this parameter doesn't exist on the action”).
  The check only fires when the action resolves to a project class whose properties are known, so an
  unresolved action never produces a false warning. The same works from a
  <code>&lt;field name="…"&gt;</code> inside a <code>*-validation.xml</code> — go-to jumps to the
  bound action's property, and an unknown field name is flagged the same way. Properties inherited
  from a project <code>BaseAction</code> are resolved up the <code>extends</code> chain, so they are
  never mis-flagged. A <strong>public field</strong> counts as a property — OGNL reads fields, and
  the parameter bags a legacy action carries are usually a nested class of public fields with no
  accessor in sight — so go-to lands on the field's declaration and the warning leaves it alone.
</p>
<p>
  <strong>Not every <code>name=</code> is a property.</strong> Struts spells several unrelated
  ideas the same way, and only the form controls bind one:
</p>
<table class="doc-table">
  <thead><tr><th>Tag</th><th>What <code>name=</code> is</th></tr></thead>
  <tbody>
    <tr><td><code>&lt;s:textfield&gt;</code>, <code>&lt;s:select&gt;</code>, <code>&lt;s:hidden&gt;</code>, …</td><td>a <strong>property</strong> of the action</td></tr>
    <tr><td><code>&lt;s:text name="label.user"/&gt;</code></td><td>a key in a <strong>resource bundle</strong></td></tr>
    <tr><td><code>&lt;s:i18n name="…"&gt;</code></td><td>the <strong>bundle</strong> itself</td></tr>
    <tr><td><code>&lt;s:action name="…"/&gt;</code></td><td>an <strong>action</strong> to invoke</td></tr>
    <tr><td><code>&lt;s:bean name="com.acme.X"&gt;</code></td><td>a <strong>class</strong></td></tr>
    <tr><td><code>&lt;s:param name="…"/&gt;</code></td><td>the parameter's own name — its <code>value=</code> is the expression</td></tr>
    <tr><td><code>&lt;s:form name="…"&gt;</code></td><td>the HTML element's name</td></tr>
  </tbody>
</table>
<p>
  The one that forces the distinction is <code>text</code>: Struts 1 writes
  <code>&lt;html:text property="user"/&gt;</code>, a text input, and Struts 2 writes
  <code>&lt;s:text name="label.user"/&gt;</code>, a lookup — one local name, opposite meanings. So
  which prefix a page bound to Struts is read from the page's own
  <code>&lt;%@ taglib %&gt;</code> lines rather than assumed, and a tag from a library Bennu does
  not recognise is left alone rather than guessed at.
</p>
<p>
  For a <strong>view JSP</strong> with no form — just OGNL (<code>%&lbrace;customer&rbrace;</code>,
  <code>&lt;s:property value="…"/&gt;</code>) — the editor works out which action renders it from the
  Struts result mappings (the reverse of action → view). When the mappings settle on one answer it's
  used automatically — that means one action, and also <em>several actions sharing one implementation
  class</em>, which is what a page reachable through three routes looks like and is not an ambiguity:
  the properties come from the class. When they genuinely disagree (or you want to override), an
  <strong>action picker</strong> in the toolbar lets you pin one, remembered per file; it lists one
  row per class, with the routes that reach it underneath. The bound action drives <kbd>Ctrl</kbd> +
  <kbd>B</kbd> on an OGNL reference and its “unknown property” warning. Only plain
  <code>%&lbrace;…&rbrace;</code> value-stack roots are checked — EL <code>$&lbrace;…&rbrace;</code>
  scoped attributes and <code>#</code>-prefixed context / iterator variables are left alone.
</p>
<p>
  Go-to and hover <strong>follow the whole path</strong>, not just its head. On
  <code>%&lbrace;ordine.cliente.nome&rbrace;</code> — or a field named the same way —
  <kbd>Ctrl</kbd> + <kbd>B</kbd> on <code>cliente</code> opens it on <code>Ordine</code>, and on
  <code>nome</code> it opens it on <code>Cliente</code>: each segment is resolved on the class the
  one before it is declared to be, with <code>List&lt;T&gt;</code> and the other single-argument
  wrappers seen through. A type name is resolved the way Java resolves it — the declaring file's
  own <strong>nested classes</strong> first, then its imports, then its package — which is what
  tells one action's inner <code>JspParam</code> from the nine others a legacy project declares.
  The walk stops rather than guesses — at a property with no accessor, at a type the project has
  no source for (a JDK or library class), at a name that means several things with nothing to say
  which — and a stopped walk simply does nothing, never a jump to the wrong file.
  The “unknown property” warning still only judges the <em>first</em> segment: the rest depend on
  types a legacy tree often cannot resolve, and a warning is held to a stricter standard than a
  jump you asked for.
</p>
<p>
  This works from a <strong>page variable</strong> too. Inside
  <code>&lt;s:iterator value="%&lbrace;elencoBandi&rbrace;" var="bando"&gt;</code> — or a
  <code>&lt;c:forEach items="…" var="…"&gt;</code>, or a <code>&lt;c:set&gt;</code> — the
  declaration is the only place the page says what the variable holds, so it is read: the
  expression is resolved against the action, the container is seen through (a
  <code>List&lt;Bando&gt;</code> makes the variable a <code>Bando</code>), and
  <code>%&lbrace;bando.titolo&rbrace;</code> follows from there. A variable declared from
  something that is not a plain path — a call, a comparison — stays untyped rather than guessed.
  Holding <kbd>Ctrl</kbd> underlines the <em>segment</em> under the pointer rather than the whole
  chain, so what a click will open is what is underlined.
</p>
<p>
  <strong>Most Struts attributes are expressions without saying so.</strong>
  <code>&lt;s:iterator value="comunicazioni.dati"&gt;</code> and
  <code>&lt;s:if test="showRiferimento"&gt;</code> carry no <code>%&lbrace;…&rbrace;</code> and are
  OGNL all the same — the wrapper is only needed on the attributes Struts declares as strings,
  which is backwards from what a reader expects and is why so much legacy markup has none. So
  go-to follows those too: <kbd>Ctrl</kbd> + <kbd>B</kbd> on <code>comunicazioni</code> or on
  <code>dati</code> lands on the property, on the class the segment before it leads to. Only for
  tags from a library the page bound to Struts, and only for the attributes Struts actually
  evaluates — <code>&lt;c:if test="…"&gt;</code> is JSTL, a different language, and is left alone.
  The <em>warning</em> stays on <code>%&lbrace;…&rbrace;</code> only: a go-to that resolves nothing
  does nothing, while a warning is a claim.
</p>
<p>
  <strong>Inside a loop, the value stack is deeper.</strong>
  <code>&lt;s:iterator value="comunicazioni.dati"&gt;</code> pushes the current element on top, so
  a bare name written underneath it — <code>%&lbrace;codice&rbrace;</code> — is a property of
  <em>that element</em> before it is anything of the action's. Go-to resolves top down: the
  innermost element, then each enclosing one, then the action, stopping at the level that actually
  declares the name. Nested loops work the same way, and a nested iterator's own expression is
  read against its parent's element, which is what it means.
</p>
<p>
  The same fact makes the check quieter, deliberately. A name inside a loop whose element type
  <em>could not</em> be resolved is a name about which nothing is known, so nothing is said about
  it — “I cannot see that type” is not evidence that a property is missing. Where the element type
  does resolve, the check keeps working inside the loop and judges the name against every level of
  the stack.
</p>
<p>
  This follows <strong>includes</strong>: an included fragment (<code>.jspf</code>) that a view page
  pulls in has no action of its own, so it <strong>inherits</strong> the action(s) of the page(s) that
  include it (transitively). So the picker, the go-to and the “unknown property” warning all work on a
  child fragment too — its fields (even those that belong to a form declared in the parent page) and
  its OGNL are checked against the parent view's action.
</p>
<h2>Struts validation files</h2>
<p>
  On a project that uses <strong>Struts</strong>, a Java <strong>action class</strong> gets a
  <strong>Validation</strong> button on the toolbar
  (also in the Command Palette): it creates the class's <code>&lt;Class&gt;-validation.xml</code>
  next to it — following the Struts naming convention — from a proper DTD-headed skeleton if it
  doesn't exist yet, then opens it. If it already exists, it just opens it.
</p>
<p>
  On a <code>&lt;Action&gt;-validation.xml</code> the toolbar shows <strong>Validators</strong>,
  which opens the <strong>chain builder</strong>. Pick a field (the action's writable properties are
  offered as chips) and stack an ordered <strong>chain</strong> of validators on it — add, remove
  and reorder them, each with its own parameters, message and <strong>short-circuit</strong> flag
  (stop the chain on first failure). Validator types and their parameters come from the built-in
  Struts catalog; a live preview shows the exact XML. <strong>Add to file</strong> appends the chain
  into the document — creating the <code>&lt;field&gt;</code> or growing an existing one — so you
  never place a caret by hand.
</p>
