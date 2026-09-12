<script lang="ts">
  /**
   * Structural search over JSP: markup patterns, holes inside EL and OGNL, the shapes worth knowing, and Java in JSP for
   * the scriptlets.
   */
  import Callout from '$lib/components/shared/ui/Callout.svelte';
</script>

<span class="eyebrow">Structural search</span>
<h1>Searching pages</h1>

<p class="doc-lead">
  The same query language pointed at JSP rather than at Java — the markup, and the Java embedded in it, which are two different searches over one file.
</p>

<h2>Markup</h2>
<p>
  A legacy Struts codebase keeps as much logic in JSPs as in classes, and text search is even weaker there: the same tag is written across four lines as
  often as one, so grepping <code>&lt;s:property value=</code> finds a fraction of them. Switch the picker to <strong>JSP</strong> and the pattern is markup:
</p>
<pre><code>&lt;s:property $pre...$ value="$x$" $post...$/&gt;
group $x$</code></pre>
<p>
  That counts every property the pages print, one row per name. A pattern needs no wrapper — any run of tags and text is already a legal page — so what you
  type is what is parsed.
</p>
<Callout variant="tip" title="The two runs are the idiom, not clutter">
  A tag's attributes are matched in order and in full, because the engine compares children and has no notion of a set. So
  <code>&lt;s:property value="$x$"/&gt;</code> alone finds only tags whose one and only attribute is <code>value</code>; <code>$pre...$</code> and
  <code>$post...$</code> let the others be there, in any order, as real pages are written. Every JSP template in the <strong>Templates</strong> menu is
  written that way.
</Callout>

<h2>Inside an expression</h2>
<p>
  An EL or OGNL body is not one blob: a <em>path</em> — a name and what is read off it — is a subtree, and operators, literals and spacing are its siblings.
  So a hole can sit inside one — but the pattern must still match the <em>whole</em> expression:
</p>
<table>
  <thead><tr><th>Pattern</th><th>Finds</th></tr></thead>
  <tbody>
    <tr><td><code>%&#123;#session.$prop$&#125;</code></td><td>Only expressions that are exactly that — not <code>%&#123;#session.user != null&#125;</code>, which has three more parts</td></tr>
    <tr><td><code>%&#123;$pre...$ #session.$prop$ $post...$&#125;</code></td><td>All of them — the same idiom as for attributes, giving what you do not name somewhere to go</td></tr>
  </tbody>
</table>
<p>
  <code>$&#123;</code> in a pattern is EL, not a hole: a placeholder name cannot begin with a brace, so the <code>$</code> is literal and needs no escaping.
  <code>$$</code> is still a literal <code>$</code> anywhere else.
</p>

<h2>Worth knowing before you discover it</h2>
<dl class="meta-grid">
  <dt>Self-closing is another shape</dt>
  <dd><code>&lt;s:property …/&gt;</code> does not match <code>&lt;s:property …&gt;&lt;/s:property&gt;</code>; write the one the page uses.</dd>
  <dt>Scriptlets are single tokens</dt>
  <dd>
    <code>$&#123;…&#125;</code>, <code>#&#123;…&#125;</code> and <code>%&#123;…&#125;</code> have structure inside; the <code>&lt;% … %&gt;</code> family —
    scriptlets, directives, declarations — is one token each. <code>&lt;%@ taglib prefix="$p$" %&gt;</code> compiles and matches nothing, because the hole
    is characters inside a leaf. For that Java there is a language of its own — below.
  </dd>
  <dt>Java-only features</dt>
  <dd><code>use of</code> is refused with a message in a page, and <code>@type</code> / <code>@value</code> have no resolver behind them, so they report <em>undecided</em> rather than filter.</dd>
</dl>

<h2>The Java inside the pages</h2>
<p>
  The third setting of the picker, <strong>Java in JSP</strong>, answers that limit: the query is <strong>Java</strong>, the files walked are the pages, and
  what is matched is the contents of their <code>&lt;% … %&gt;</code>, <code>&lt;%= … %&gt;</code> and <code>&lt;%! … %&gt;</code> blocks.
</p>
<pre><code>session.getAttribute($key$)
group $key$</code></pre>
<p>
  Every key the pages read out of the session, one row per key — a question a JSP query cannot ask, since to the page grammar a scriptlet is a token. The
  query language does not change: the same holes, constraints and clauses, <code>use of</code> included.
</p>
<p>
  Each block is lifted out and wrapped in the smallest legal Java that makes its <em>kind</em> parse — a scriptlet is statements, a declaration is members, a
  <code>&lt;%= %&gt;</code> is an expression — then matched and mapped back onto the page. A hit's line, its preview and where a click takes you are the code
  as written, never the scaffolding.
</p>
<Callout variant="info" title="Two consequences of the lifting">
  <strong>Type constraints come back undecided</strong>: the resolver is asked about a file, and a wrapped scriptlet is in no file. And
  <strong><code>group enclosing</code> has nothing to name</strong> — the page is the method. Use <code>group file</code>.
</Callout>
