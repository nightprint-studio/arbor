<!-- Bennu docs — MyBatis mappers: the XML and the interface, navigated as one thing. -->
<h1>MyBatis mappers</h1>
<p class="doc-lead">
  A mapper is an interface with no implementation and an XML file with no compiler checking it.
  Bennu links the two.
</p>

<h2>MyBatis mappers</h2>
<p>
  Inside a mapper <code>.xml</code>, <kbd>Ctrl</kbd> + <kbd>B</kbd> (or <kbd>Ctrl</kbd> + click) follows
  the piece under the caret: a statement <code>id="…"</code> jumps to the matching method on the
  mapper interface (<code>.java</code>); the mapper <code>namespace="…"</code> opens that interface;
  an <code>&lt;include refid="…"&gt;</code> jumps to the <code>&lt;sql&gt;</code> fragment it pulls in;
  and a statement's <code>resultMap="…"</code> jumps to the <code>&lt;resultMap&gt;</code> it uses.
  Fragment references within the same file resolve instantly (no index needed).
</p>
