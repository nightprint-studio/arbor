<!-- Bennu docs — tag libraries: reading .tld files, including the ones inside jars. -->
<h1>Tag libraries</h1>
<p class="doc-lead">
  The tags you write come from <code>.tld</code> files, and most of them live inside dependency
  jars. Bennu reads both, so a taglib tag stops being opaque text.
</p>

<h3>The tag libraries themselves</h3>
<p>
  Bennu reads the <code>.tld</code> files a page declares — the project's own, and the ones
  inside the <strong>dependency jars</strong>, which is where the tags you actually write come
  from. So a taglib tag stops being opaque text:
</p>
<ul>
  <li><strong>Completion.</strong> <code>&lt;s:</code> lists that library's tags; inside a tag,
    its own attributes, minus the ones already written. Typing a <code>uri="…"</code> in a
    directive completes from every library the project can resolve.</li>
  <li><strong>Hover</strong> carries the TLD's own prose — the tag's description, an attribute's
    type, whether it is required, whether it accepts a runtime expression. On a legacy library
    that is often the only documentation there is.</li>
  <li><kbd>Ctrl</kbd> + <kbd>B</kbd> (or <kbd>Ctrl</kbd> + click) on the <code>uri</code> opens
    the <strong>TLD</strong> — including one that lives inside a jar; on a tag name it lands on
    that tag's <code>&lt;tag&gt;</code> declaration, and on an attribute name on the
    <code>&lt;attribute&gt;</code>.</li>
  <li><strong>Checks:</strong> a tag the library does not declare, an attribute it does not have,
    a required attribute that is missing, a <code>uri</code> nothing on the classpath ships.</li>
</ul>
<p>
  All of it stays silent where it cannot be sure. A project whose dependencies have not resolved
  yet reports nothing rather than reporting everything; a prefix the page never declared is never
  flagged, because it usually comes from an included fragment and the include is invisible from
  the page; and a tag declaring <code>dynamic-attributes</code> — or one written as a
  <code>.tag</code> file — has an attribute list that is <em>unknown</em> rather than empty.
</p>
<p>
  A <code>.tld</code> is itself edited with completion and checks, against a tag-library grammar
  that is <strong>built in</strong>: a TLD names its schema at <code>java.sun.com</code> and the
  only copy sits inside a servlet container's jars, which are <code>provided</code> scope and
  frequently absent — so the file defining a project's entire tag vocabulary was the one XML file
  with no vocabulary of its own. Both generations are covered, the JSP 1.1 spellings
  (<code>tagclass</code>, <code>bodycontent</code>) beside the modern ones.
</p>
