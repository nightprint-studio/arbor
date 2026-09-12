<script lang="ts">
  /**
   * JSP pages: the grammar, EL and OGNL expressions, script blocks with their own completion and hover, JSP markers inside
   * JavaScript, and per-taglib colours.
   */
  import Callout from '$lib/components/shared/ui/Callout.svelte';
</script>

<span class="eyebrow">Java &amp; JSP</span>
<h1>JSP pages</h1>

<p class="doc-lead">
  A JSP is not HTML with noise in it. The taglib tags, the scriptlets and the EL and OGNL expressions are each parsed as what they are — which is what makes a name
  inside <code>&#36;&#123;…&#125;</code> something you can navigate.
</p>

<h2>The grammar</h2>
<p>
  A dedicated grammar colours namespaced taglib tags (<code>&lt;s:iterator&gt;</code>, <code>&lt;c:if&gt;</code>), scriptlets, EL <code>$&lbrace;…&rbrace;</code> and
  OGNL <code>%&lbrace;…&rbrace;</code>. Navigating a Struts application from its pages is <strong>Struts navigation</strong>.
</p>

<h2>Expressions are parsed</h2>
<p>
  An EL or OGNL expression is not one block. A <em>path</em> — a name and what is read off it, <code>#session.currentUser</code>, <code>items[0].price</code> — is a
  construct of its own, with identifiers, property accesses, strings, numbers, operators and keywords each coloured.
</p>
<ul>
  <li>The <code>#</code> of an OGNL context reference is marked apart from the name after it: <code>#session</code> is precisely <em>not</em> a property of the action.</li>
  <li>The syntax tree shows that structure, and a structural search can put a hole inside an expression — <code>%&#123;#session.$prop$&#125;</code>.</li>
</ul>
<Callout variant="tip" title="An unfinished expression stops at the next tag">
  An expression that does not parse — every line while it is being typed — is left plain and <strong>stops at the next tag</strong>: an unclosed
  <code>$&#123;</code> never swallows the rest of the page.
</Callout>

<h2>Script blocks</h2>
<p>
  A <code>&lt;script&gt;</code> body is read as real JavaScript: object keys, member accesses and call sites each have a colour; numbers in every form — hex, binary,
  exponents, separators — are numbers; template literals colour their <code>$&lbrace;…&rbrace;</code> holes as code; <code>this</code> stands out; and a regular
  expression is told from a division, which keeps one <code>/</code> from painting the rest of the line as a literal.
</p>
<p>
  <strong>Completion and hover work inside it too</strong>, answered by Bennu itself — no language server will ever serve a JSP, a template that <em>prints</em> JavaScript:
</p>
<div class="feature-grid two-col">
  <div class="feature-card">
    <div class="fc-eyebrow">What the block declares</div>
    <div class="fc-title">Its own names</div>
    <div class="fc-desc"><code>var</code>, <code>function</code>, the <code>foo: function (…)</code> form a jQuery-era page is mostly written in, and their parameters — each hovering with the line it was declared on.</div>
  </div>
  <div class="feature-card">
    <div class="fc-eyebrow">By reflection</div>
    <div class="fc-title">The platform</div>
    <div class="fc-desc">After <code>document.</code>, <code>location.</code> or <code>Math.</code>, the members the object really has, read in the engine the page runs in — a list that cannot be stale.</div>
  </div>
</div>
<Callout variant="info" title="It stops at jQuery">
  <code>$</code> is offered as a name, but there is no loaded library to read <code>$(…).</code> off.
</Callout>

<h2>JSP inside the JavaScript</h2>
<p>
  The script body is a <strong>template that produces JavaScript</strong>, and it is read as one. A scriptlet, a <code>&lt;%= … %&gt;</code>, an EL or OGNL expression
  and a whole namespaced taglib tag are recognised as what they are — <strong>inside a string</strong> too, which is where it matters:
</p>
<pre><code>errore = "&lt;wp:i18n key="LABEL_REQUIRED_COMUNE" /&gt;";</code></pre>
<p>
  Read as plain JavaScript, the tag's own quote closes the string and the rest of the line is coloured as something it is not. The marker wins instead — which is what
  the server does, since substitution happens before there is any JavaScript to quote, and the rule the page grammar applies to attribute values.
</p>
<p>
  The whole marker takes the JSP colour rather than being coloured inside: within JavaScript, <em>this part is not JavaScript</em> is the useful thing to say. A marker
  spanning several lines is followed across them.
</p>

<h2>A colour per taglib</h2>
<p>
  <strong>Each taglib gets its own colour</strong>, and its <code>&lt;%@ taglib %&gt;</code> line wears the same one — so the declarations at the top of the page are the
  legend for everything below them, and Struts, JSTL and Entando tags are told apart at a glance.
</p>
<ul>
  <li>A prefix keeps its colour across every file that declares it; two prefixes in one page never share one.</li>
  <li>A prefix the page never declared stays the plain tag colour — the quickest way to notice a missing directive, since the server will not render it either.</li>
</ul>
