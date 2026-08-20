<!-- Bennu docs — structural search over JSP pages and the Java inside them. -->
<h1>Searching pages</h1>
<p class="doc-lead">
  The same query language pointed at JSP rather than at Java — the markup, and the Java embedded
  inside it, which are two different searches over one file.
</p>

<h2>Searching pages</h2>
<p>
  A legacy Struts codebase keeps as much of its logic in JSPs as in classes, and a text search is
  even weaker there than over Java — the same tag is written across four lines as often as one,
  so grepping <code>&lt;s:property value=</code> finds a fraction of them. Switch the picker to
  <strong>JSP</strong> and the pattern is markup:
</p>
<pre><code>&lt;s:property $pre...$ value="$x$" $post...$/&gt;
group $x$</code></pre>
<p>
  That counts every property the pages print, one row per name. A pattern needs no wrapper here —
  any run of tags and text is already a legal page — so what you type is what is parsed.
</p>
<p>
  <strong>Expressions have structure too.</strong> An EL or OGNL body is not one blob: a
  <em>path</em> — a name and what is read off it — is a subtree, and operators, literals and
  spacing are its siblings. So a hole can sit inside one.
</p>
<p>
  The catch is that a pattern still has to match the <em>whole</em> expression.
  <code>%&#123;#session.$prop$&#125;</code> finds only the expressions that are exactly that —
  not <code>%&#123;#session.user != null&#125;</code>, which has three more parts. Put a run on
  each side and it finds them all:
</p>
<pre><code>%&#123;$pre...$ #session.$prop$ $post...$&#125;
group $prop$</code></pre>
<p>
  Same idiom as the one for attributes, and for the same reason — the engine compares children,
  so anything you do not name has to be given somewhere to go.
</p>
<p>
  <code>$&#123;</code> in a pattern is EL and not a hole: a placeholder name cannot begin with a
  brace, so the <code>$</code> is read literally and needs no escaping.
  <code>$$</code> is still a literal <code>$</code> anywhere else.
</p>
<p>
  <strong>The two runs are the idiom, not clutter.</strong> A tag’s attributes are matched in
  order and in full, because the engine compares children and has no notion of a set. So
  <code>&lt;s:property value="$x$"/&gt;</code> alone finds only the tags whose one and only
  attribute is <code>value</code>; <code>$pre...$</code> and <code>$post...$</code> let the rest
  of them be there, in any order, which is how real pages are written. Every JSP template in the
  <strong>Templates</strong> menu is written that way.
</p>
<p>Three more things worth knowing before you discover them:</p>
<ul>
  <li>
    A <strong>self-closing tag and an open/close pair are different shapes</strong>.
    <code>&lt;s:property …/&gt;</code> does not match
    <code>&lt;s:property …&gt;&lt;/s:property&gt;</code>; write the one the page uses.
  </li>
  <li>
    <strong>Expressions are decomposed; scriptlets are not.</strong>
    <code>$&#123;…&#125;</code> / <code>#&#123;…&#125;</code> / <code>%&#123;…&#125;</code> have
    real structure inside, so <code>%&#123;#session.$prop$&#125;</code> is a pattern that works.
    The <code>&lt;% … %&gt;</code> family — scriptlets, directives, declarations — is still a
    <strong>single token</strong> each: <code>&lt;%@ taglib prefix="$p$" %&gt;</code> compiles
    and then matches nothing, because the hole is characters inside a leaf rather than a node.
    For the Java in the pages there is a language of its own — see below.
  </li>
  <li>
    <code>use of</code> and the <code>@type</code> / <code>@value</code> constraints ask about
    Java code. In a page the first is refused with a message rather than run, and the second has
    no resolver behind it, so it reports <em>undecided</em> instead of filtering.
  </li>
</ul>
<h2>The Java inside the pages</h2>
<p>
  The third setting of the picker — <strong>Java in JSP</strong> — is the answer to the limit
  above. The query is <strong>Java</strong>, the files walked are the pages, and what is matched
  is the contents of their <code>&lt;% … %&gt;</code>, <code>&lt;%= … %&gt;</code> and
  <code>&lt;%! … %&gt;</code> blocks.
</p>
<pre><code>session.getAttribute($key$)
group $key$</code></pre>
<p>
  That is every key the pages read out of the session, one row per key — a question a JSP query
  cannot ask at all, because to the page grammar a scriptlet is a single token. Nothing about the
  query language changes: it is an ordinary Java pattern, with the same holes, constraints and
  clauses, including <code>use of</code>.
</p>
<p>
  Each block is lifted out and wrapped in the smallest legal Java that makes its <em>kind</em>
  parse — a scriptlet is statements, a declaration is members, a <code>&lt;%= %&gt;</code> is an
  expression — then matched and mapped back onto the page. So a hit's line, its preview and where
  a click takes you are all the code as it is written, never the scaffolding it was parsed inside.
</p>
<p>
  Two things follow from the lifting and are worth knowing. <strong>Type constraints come back
  undecided</strong>: the resolver is asked about a file, and a scriptlet wrapped in a synthetic
  class is in no file — reporting <em>undecided</em> is this tool's word for "unknown", and it is
  the only honest one here. And <strong><code>group enclosing</code> has nothing to name</strong>:
  the block has no enclosing method, the page is the method. Use <code>group file</code>.
</p>
