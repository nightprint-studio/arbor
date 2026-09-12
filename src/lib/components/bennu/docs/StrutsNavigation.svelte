<script lang="ts">
  /**
   * Struts navigation: actions, includes and page variables; form fields as action properties; the view JSP's action;
   * following whole OGNL paths, loop elements and implicit OGNL attributes; fragments; and validation files.
   */
  import Callout from '$lib/components/shared/ui/Callout.svelte';
</script>

<span class="eyebrow">Java &amp; JSP</span>
<h1>Struts navigation</h1>

<p class="doc-lead">
  In a Struts application the behaviour is spread across XML rather than expressed in code. These are the jumps that put it back together: an <code>action="…"</code> string
  on one side, a configuration entry, a class and a view on the other.
</p>

<h2>Actions, includes and page variables</h2>
<table>
  <thead><tr><th>The caret on</th><th><kbd>Ctrl</kbd> + <kbd>B</kbd></th><th><kbd>Alt</kbd> + <kbd>F7</kbd></th></tr></thead>
  <tbody>
    <tr><td>An <code>action="…"</code> reference</td><td>The <code>&lt;action&gt;</code> config, its view JSP, or the action class</td><td>Every JSP using it</td></tr>
    <tr><td>A page variable's reference — from <code>&lt;c:set var&gt;</code>, <code>&lt;s:set var&gt;</code>, <code>&lt;c:forEach var&gt;</code>, <code>&lt;s:iterator var&gt;</code></td><td>Where the variable is set</td><td>Every reference in the page — it is page-scoped</td></tr>
    <tr><td>An include path — <code>&lt;%@ include file&gt;</code>, <code>&lt;jsp:include page&gt;</code>, <code>&lt;s:include value&gt;</code></td><td>The referenced JSP</td><td>—</td></tr>
  </tbody>
</table>
<p>What is flagged, with a warning squiggle:</p>
<ul>
  <li>An absolute action reference resolving to nothing — never a wildcard or a runtime <code>$&lbrace;…&rbrace;</code> / <code>%&lbrace;…&rbrace;</code> one.</li>
  <li>A static include whose file does not exist — <code>&lt;%@ include file&gt;</code>, <code>&lt;jsp:include page&gt;</code>, <code>&lt;s:include value&gt;</code>,
    <code>&lt;c:import url&gt;</code> — never a computed or <code>http(s)://</code> one.</li>
</ul>
<p>
  Inline <code>&lt;script&gt;</code> and <code>&lt;style&gt;</code> blocks are highlighted as JavaScript and CSS. And while editing a JSP, the toolbar's <strong>Insert
  tag</strong> menu drops a ready-made snippet at the caret — <code>&lt;c:set&gt;</code>, <code>&lt;s:set&gt;</code>, <code>&lt;s:property&gt;</code>,
  <code>&lt;s:iterator&gt;</code>, <code>&lt;c:forEach&gt;</code>, <code>&lt;s:if&gt;</code> / <code>&lt;c:if&gt;</code>, <code>&lt;s:url&gt;</code>,
  <code>&lt;s:text&gt;</code>, <code>&lt;s:textfield&gt;</code> — with placeholder attributes to overtype.
</p>

<h2>Form fields are action properties</h2>
<p>
  The <code>name="…"</code> of an <code>&lt;s:textfield&gt;</code>, <code>&lt;input&gt;</code> or <code>&lt;s:select&gt;</code> inside a form is a property of the form's
  <strong>action class</strong>:
</p>
<ul>
  <li><kbd>Ctrl</kbd> + <kbd>B</kbd> jumps to its <code>get</code>, <code>set</code> or <code>is</code> accessor in the action.</li>
  <li>A name that is <strong>not</strong> a property gets a warning — a likely typo, "this parameter does not exist on the action".</li>
  <li>The same works from a <code>&lt;field name="…"&gt;</code> in a <code>*-validation.xml</code>.</li>
  <li>Properties inherited from a project <code>BaseAction</code> are found up the <code>extends</code> chain, and a <strong>public field</strong> counts as a property —
    OGNL reads fields, and a legacy action's parameter bags are often a nested class of public fields with no accessor.</li>
</ul>
<Callout variant="info" title="Never a false warning">
  The check fires only when the action resolves to a project class whose properties are known.
</Callout>
<p><strong>Not every <code>name=</code> is a property.</strong> Struts spells several unrelated ideas the same way, and only form controls bind one:</p>
<table>
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
<Callout variant="warning" title="text means two opposite things">
  Struts 1 writes <code>&lt;html:text property="user"/&gt;</code>, a text input; Struts 2 writes <code>&lt;s:text name="label.user"/&gt;</code>, a lookup. So which prefix a page
  bound to Struts is read from its own <code>&lt;%@ taglib %&gt;</code> lines, never assumed — and a tag from a library Bennu does not recognise is left alone.
</Callout>

<h2>A view JSP and its action</h2>
<p>
  A view with no form — just OGNL, <code>%&lbrace;customer&rbrace;</code>, <code>&lt;s:property value="…"/&gt;</code> — has its action worked out from the Struts result
  mappings, the reverse of action → view. That action drives <kbd>Ctrl</kbd> + <kbd>B</kbd> on an OGNL reference and its "unknown property" warning.
</p>
<dl class="meta-grid">
  <dt>One answer</dt>
  <dd>Used automatically — one action, and also <em>several actions sharing one class</em>, which is a page reachable through three routes, not an ambiguity: the properties come from the class.</dd>
  <dt>Disagreeing answers</dt>
  <dd>An <strong>action picker</strong> in the toolbar pins one, remembered per file — one row per class, with the routes reaching it underneath. It also overrides a single answer.</dd>
</dl>
<p>
  Only plain <code>%&lbrace;…&rbrace;</code> value-stack roots are checked — EL <code>$&lbrace;…&rbrace;</code> scoped attributes and <code>#</code>-prefixed context or iterator
  variables are left alone.
</p>

<h2>Following the whole path</h2>
<p>
  Go-to and hover <strong>follow every segment</strong>, not only the head. On <code>%&lbrace;ordine.cliente.nome&rbrace;</code> — or a field named that way —
  <kbd>Ctrl</kbd> + <kbd>B</kbd> on <code>cliente</code> opens it on <code>Ordine</code>, and on <code>nome</code> on <code>Cliente</code>:
</p>
<ul>
  <li>each segment is resolved on the class the one before it is declared as, with <code>List&lt;T&gt;</code> and other single-argument wrappers seen through;</li>
  <li>a type name resolves as Java does — the declaring file's <strong>nested classes</strong> first, then its imports, then its package — which tells one action's inner
    <code>JspParam</code> from the nine others a legacy project declares;</li>
  <li>holding <kbd>Ctrl</kbd> underlines the <em>segment</em> under the pointer, so what a click opens is what is underlined.</li>
</ul>
<Callout variant="info" title="It stops rather than guesses">
  At a property with no accessor, a type with no project source (a JDK or library class), a name meaning several things — a stopped walk does nothing, never a jump to the wrong
  file. The "unknown property" warning still judges only the <em>first</em> segment: the rest depend on types a legacy tree often cannot resolve, and a warning answers to a
  stricter standard than a jump you asked for.
</Callout>

<h3>From a page variable</h3>
<p>
  Inside <code>&lt;s:iterator value="%&lbrace;elencoBandi&rbrace;" var="bando"&gt;</code> — or a <code>&lt;c:forEach items var&gt;</code>, or a <code>&lt;c:set&gt;</code> — the
  declaration is the only place saying what the variable holds, so it is read: the expression is resolved against the action, the container seen through (a
  <code>List&lt;Bando&gt;</code> makes the variable a <code>Bando</code>), and <code>%&lbrace;bando.titolo&rbrace;</code> follows. A variable declared from anything but a plain
  path — a call, a comparison — stays untyped.
</p>

<h3>Attributes that are OGNL without saying so</h3>
<p>
  <code>&lt;s:iterator value="comunicazioni.dati"&gt;</code> and <code>&lt;s:if test="showRiferimento"&gt;</code> carry no <code>%&lbrace;…&rbrace;</code> and are OGNL all the
  same — the wrapper is needed only on attributes Struts declares as strings, backwards from what a reader expects. So go-to follows those too, for tags from a library bound to
  Struts and only the attributes Struts evaluates. <code>&lt;c:if test="…"&gt;</code> is JSTL, another language, and is left alone. The <em>warning</em> stays on
  <code>%&lbrace;…&rbrace;</code> only: a go-to resolving nothing does nothing, while a warning is a claim.
</p>

<h3>Inside a loop, the value stack is deeper</h3>
<p>
  <code>&lt;s:iterator value="comunicazioni.dati"&gt;</code> pushes the current element on top, so a bare name under it — <code>%&lbrace;codice&rbrace;</code> — is a property of
  <em>that element</em> before it is anything of the action's. Go-to resolves top down: the innermost element, each enclosing one, then the action, stopping where the name is
  declared. A nested iterator's own expression is read against its parent's element.
</p>
<Callout variant="info" title="Quieter inside a loop, on purpose">
  A name in a loop whose element type <em>could not</em> be resolved is one about which nothing is known, so nothing is said — "I cannot see that type" is no evidence a property
  is missing. Where the element type resolves, the check judges the name against every level of the stack.
</Callout>

<h3>Included fragments</h3>
<p>
  A fragment (<code>.jspf</code>) a view pulls in has no action of its own, so it <strong>inherits</strong> the actions of the pages including it, transitively. The picker, go-to
  and the warning all work there: its fields — even those belonging to a form declared in the parent — and its OGNL are checked against the parent view's action.
</p>

<h2>Struts validation files</h2>
<ol class="step-list">
  <li>On a Java <strong>action class</strong> in a Struts project, the toolbar's <strong>Validation</strong> button — also in the command palette — creates
    <code>&lt;Class&gt;-validation.xml</code> beside it, from a DTD-headed skeleton following the Struts naming convention, and opens it. If it exists, it just opens it.</li>
  <li>On that file, <strong>Validators</strong> opens the <strong>chain builder</strong>.</li>
  <li>Pick a field — the action's writable properties are offered as chips — and stack an ordered <strong>chain</strong>: add, remove and reorder validators, each with its
    parameters, message and <strong>short-circuit</strong> flag, stopping the chain on first failure. Types and parameters come from the built-in Struts catalogue, and a live
    preview shows the XML.</li>
  <li><strong>Add to file</strong> appends the chain — creating the <code>&lt;field&gt;</code> or growing an existing one — so you never place a caret by hand.</li>
</ol>
