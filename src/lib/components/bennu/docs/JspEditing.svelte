<!-- Bennu docs — JSP pages: the grammar, EL/OGNL, and what resolves inside a page. -->
<h1>JSP pages</h1>
<p class="doc-lead">
  A JSP is not treated as HTML with noise in it. The taglib tags, the scriptlets and the
  EL / OGNL expressions are each parsed as what they are, which is what makes a name inside
  <code>&#36;&#123;…&#125;</code> something you can navigate.
</p>

<h2>JSP &amp; Struts navigation</h2>
<p>
  JSP files are highlighted by a dedicated grammar — namespaced taglib tags (<code>&lt;s:iterator&gt;</code>,
  <code>&lt;c:if&gt;</code>), scriptlets, EL <code>$&lbrace;…&rbrace;</code> and OGNL <code>%&lbrace;…&rbrace;</code> all colour
  correctly, and an EL/OGNL expression is <strong>parsed</strong> rather than treated as one
  block: a <em>path</em> — a name and what is read off it, <code>#session.currentUser</code>,
  <code>items[0].price</code> — is a construct of its own, with identifiers, property accesses,
  strings, numbers, operators and keywords each getting their own colour. The <code>#</code> of
  an OGNL context reference is marked apart from the name after it, because <code>#session</code>
  is precisely <em>not</em> a property of the action and that is worth seeing at a glance.
</p>
<p>
  That structure is what the rest of the editor reads: the syntax tree shows it, and a structural
  search can put a hole inside an expression (<code>%&#123;#session.$prop$&#125;</code>). An
  expression that does not parse — the state of every line while it is being typed — is left
  plain and, crucially, <strong>stops at the next tag</strong>: an unclosed
  <code>$&#123;</code> never swallows the rest of the page.
</p>
<p>
  A <code>&lt;script&gt;</code> body is read as real JavaScript, not just keywords-and-strings:
  object keys, member accesses and call sites are each their own colour, numbers in every form
  (hex, binary, exponents, separators) are numbers, template literals colour their
  <code>$&lbrace;…&rbrace;</code> holes as code, <code>this</code> stands out, and a regular
  expression is told apart from a division — which is what keeps one <code>/</code> from painting
  the rest of the line as a literal. The same tokenizer colours a standalone <code>.js</code> file.
</p>
<p>
  It also knows the script body is a <strong>template that produces JavaScript</strong> rather
  than JavaScript. A scriptlet, a <code>&lt;%= … %&gt;</code>, an EL or OGNL expression and a
  whole namespaced taglib tag are recognised as what they are — including
  <strong>inside a string</strong>, which is where it matters:
</p>
<pre><code>errore = "&lt;wp:i18n key="LABEL_REQUIRED_COMUNE" /&gt;";</code></pre>
<p>
  read as plain JavaScript, the tag's own quote closes the string and the rest of the line is
  coloured as something it is not. The marker wins instead, which is both what the server does —
  substitution happens before there is any JavaScript to quote — and the rule the page's grammar
  already applies to attribute values. The whole marker takes the JSP colour rather than being
  coloured inside: within a block of JavaScript, <em>this part is not JavaScript</em> is the
  useful thing to say. A marker spanning several lines is followed across them.
</p>
<p>
  <strong>Each taglib gets its own colour</strong>, and its <code>&lt;%@ taglib %&gt;</code> line wears
  the same one — so the declarations at the top of the page are the legend for everything below
  them, and Struts, JSTL and Entando tags are told apart at a glance instead of all reading as
  "a taglib tag". A prefix keeps its colour across every file that declares it; two prefixes in
  one page never share one. A prefix the page never declared stays the plain tag colour — which
  is also the quickest way to notice a missing directive, since the server won't render it either.
</p>
