<!-- Bennu docs — moving around the code: find, go-to, usages, hover. -->
<h1>Navigation</h1>
<p class="doc-lead">
  Getting to the thing you are thinking of, from wherever you are. Everything here works from the
  keyboard, and everything here answers from the index rather than from the open tabs — so it
  finds what you have never opened.
</p>

<h2>Find</h2>
<p>
  <kbd>Ctrl</kbd> + <kbd>F</kbd> searches the current file. <kbd>Ctrl</kbd> + <kbd>Shift</kbd> +
  <kbd>F</kbd> opens <strong>Find in project</strong> — a backend-powered search across the whole
  project with <strong>Match case</strong>, <strong>Whole word</strong> and <strong>Regex</strong>
  toggles beside the field (and, on a workspace, a fourth reaching into every member project),
  grouping hits by file with the match highlighted. Results stream in as the scan finds them, so a
  large project fills the list instead of making you wait for it.
</p>
<p>
  The selected hit is shown <strong>in context</strong> beside the list — the lines around it, with
  the match highlighted — which is what tells four identical-looking lines apart without opening
  four files. ↑/↓ move the selection (and the preview follows), <kbd>Enter</kbd> opens the hit.
  If a word is <strong>selected</strong> in the editor, it pre-fills the search field (both here and
  in Find-in-file).
</p>
<p>
  The header row is everything that decides <strong>what is searched</strong>. The
  <strong>Source</strong> picker — <strong>Project</strong>, <strong>Project &amp;
  dependencies</strong>, <strong>Dependencies</strong> — says whose text is read. Then the two
  narrowings: the <strong>module</strong>, on a multi-module build, and a <strong>file
  mask</strong> (<code>*.java</code>, or several at once as <code>*.jsp, *.tag</code>). Those two
  filter what came back rather than what is scanned, so changing either re-lists instantly instead
  of re-running the search, and both are <strong>remembered per project</strong>. The count
  between them says how many of the matches survived them.
</p>
<p>
  Reading the <strong>dependency jars</strong> — their XML, schemas, tag libraries and property
  files — is how you find which artifact declares the interceptor or the bean you are looking at.
  Those hits are <strong>tinted</strong> and named by their <strong>artifact</strong>, arrive
  after the project's own, and opening one extracts it read-only. It is per-search rather than a
  setting: every candidate entry has to be decompressed to be read, so it is a cost you take for
  the question you are asking now.
</p>
<h2>Go to line</h2>
<p>
  <kbd>Ctrl</kbd> + <kbd>G</kbd> opens the go-to-line box — type <code>42</code> or
  <code>42:8</code> (line:column) and press <kbd>Enter</kbd>.
</p>
<h2>Go to declaration</h2>
<p>
  Put the caret on a Java <strong>symbol</strong> — a class, method, field or local — and press
  <kbd>Ctrl</kbd> + <kbd>B</kbd> (or <kbd>Ctrl</kbd> + click, or the right-click menu) to jump to its
  declaration. If you're <strong>already on the declaration itself</strong> — a method signature, or
  the declaration of a variable, class or record — jumping would be a no-op, so the same gesture shows
  its <strong>usages</strong> instead (like IntelliJ). On a JSP form or link <strong>action
  reference</strong> — an <code>action="…"</code> value or a path like
  <code>/do/Category/viewTree</code> — it jumps to where the action is declared: the Struts config
  fragment, or its view JSP; if it resolves only to an implementation class, the class name is shown.
  It answers from the project index / config graph, so it works once the index is warm and stays quiet
  when a symbol can't be resolved.
</p>
<p>
  In a <strong>Struts config XML</strong> the same gesture works on a <code>&lt;result&gt;</code>: a
  JSP path (<code>/WEB-INF/x.jsp</code>) opens that JSP, and an OGNL/EL result (<code>$&#123;urlErrori&#125;</code>)
  jumps to the owning action's property. A JSP path that doesn't exist under the web app, or an OGNL
  root that isn't a property of the action, is flagged with a warning squiggle.
</p>
<p>
  The same gesture on a <strong>library or JDK method</strong> — <code>list.add(…)</code>,
  <code>LOGGER.info(…)</code> — opens that library's source view and lands <strong>on the method
  itself</strong>. The receiver is typed against the project's classpath, so it works on anything your
  dependencies resolve to, and it chains: from inside one library view you can go on to the next.
</p>
<p>
  Ctrl+B on a <strong>library or JDK type</strong> (one with no project source) opens a
  <strong>decompiled stub</strong> generated from its bytecode — the type declaration plus every field
  and method signature, with a header noting it's decompiled (method bodies aren't stored in a class
  file). It's cached, so opening it again is instant. A decompiled stub is a read-only view and is not
  validated (it has no bodies, so validation would only report noise).
</p>
<h2>Find usages</h2>
<p>
  In a <code>.svelte</code> file, <kbd>Alt</kbd> + <kbd>Shift</kbd> + <kbd>F7</kbd> asks for the
  usages of <strong>the component itself</strong> — the imports and the <code>&lt;Foo /&gt;</code>
  tags. It is a separate key because it is the one subject <kbd>Alt</kbd> + <kbd>F7</kbd> cannot be
  pointed at: the file is the component, so there is no name written inside it to put a caret on.
</p>
<p>
  Put the caret on a class, method or field and press <kbd>Alt</kbd> + <kbd>F7</kbd> to list every
  place it's used across the project in a popover — pick one to jump to it. It answers once the index
  is warm.
</p>
<p>
  A field counts <strong>every</strong> read of it, whether or not the code wrote a receiver:
  <code>this.count</code>, <code>other.count</code>, <code>Config.MAX</code> and the bare
  <code>count</code> that means <code>this.count</code> are the same field. That last shape is the
  usual one — and for a <code>static final</code> constant it is often the only one. A local
  variable or parameter of the same name is that variable, not the field it hides, so a
  <code>setValue(int value)</code> does not report its own parameter as a use of
  <code>this.value</code>. A declaration is never a use of itself.
</p>
<h2>Hover</h2>
<p>
  Rest the pointer on a class, method or field to see a card with what it is (a tag: class,
  interface, enum, method, field), its signature, and the type that <em>declares</em> it — the
  supertype, when you're hovering an inherited member. It answers from the project index, so it
  appears once the index is warm.
</p>
<p>
  A <strong>Javadoc</strong> on a project declaration is read rather than dumped: the prose comes
  first, then <code>@param</code>, <code>@return</code> and <code>@throws</code> as a labelled list,
  with <code>&lbrace;@link …&rbrace;</code> shown as what it names and <code>@deprecated</code>
  highlighted.
</p>
<p>
  Hovering a <strong>variable</strong> — a local, a parameter, a loop variable, a
  <code>catch</code> parameter, a pattern variable — names its type, its
  <strong>fully-qualified</strong> type (which of the four <code>Order</code>s on the classpath this
  one is) and <strong>what that type is</strong>: class, interface, enum, record or annotation. A
  <code>var</code> or a Lombok <code>val</code> never shows as <code>var</code>: the card shows the
  type the compiler deduced, including the element type in
  <code>for (val row : rows)</code>.
</p>
<p>
  In a JSP, hovering a form field, an OGNL reference or a <code>*-validation.xml</code>
  <code>&lt;field&gt;</code> shows the <strong>type</strong> of the matching property on the bound
  action class, along with the action it belongs to.
</p>
