<!-- Bennu docs — Editing & navigation. -->
<h1>Editing &amp; navigation</h1>
<p class="doc-lead">
  The editor is a fast, syntax-highlighted view of your project's files with a tab strip,
  a symbol structure, code folding, completions, project-wide search and keyboard-first navigation.
  Java gets full semantic highlighting; XML/JSP, <code>.properties</code>, YAML, JSON, Markdown,
  HTML, CSS/SCSS, JavaScript and SQL are highlighted too, so the whole legacy stack reads cleanly.
</p>

<h2>Tabs</h2>
<ul>
  <li>Open a file from the <strong>Project</strong> tree or a <strong>Find in project</strong> hit — it joins the tab strip.</li>
  <li>Switch tabs with a click; the <strong>×</strong> closes one and a neighbour takes focus.</li>
</ul>

<h2>Structure</h2>
<p>
  The <strong>Structure</strong> tool (left rail) lists the active file's symbols — types, methods and
  fields — grouped by kind and filterable, sortable by position or name. Click a symbol to jump the
  editor to its declaration. Methods carrying an <code>@Override</code> annotation show an
  <strong>override marker</strong> (an up arrow), so the members that specialise a supertype stand
  out at a glance. The header carries <strong>Collapse all</strong> / <strong>Expand all</strong>
  chevrons to fold or unfold the whole tree at once, just like the Project panel.
</p>

<h2>Project tree</h2>
<p>
  The <strong>Project</strong> panel header carries quick actions: create a new file, locate the open
  file in the tree, collapse or expand the whole tree, and an options menu. Right-clicking a file or
  folder opens a context menu (Open · Copy path · Copy relative path · Reveal).
</p>

<h2>Code folding</h2>
<p>
  Braced blocks (classes, methods, blocks) and block comments fold from the gutter chevrons; the head
  line stays visible. Folding is computed live from the syntax tree — no indexing needed.
</p>

<h2>Minimap</h2>
<p>
  A scrollable <strong>minimap</strong> in the right gutter gives a bird's-eye overview of the whole
  file — drag or click it to jump. Toggle it in <strong>Settings → Editor → Minimap</strong>.
</p>

<h2>Emmet</h2>
<p>
  In JSP and HTML files, type an <strong>Emmet abbreviation</strong> and press <kbd>Tab</kbd> to
  expand it into markup — <code>ul>li.item*3</code> becomes a list, <code>div#app</code> a div with
  an id, <code>a[href]</code> a link. When the caret isn't on a valid abbreviation, <kbd>Tab</kbd>
  just indents as usual.
</p>

<h2>Completions</h2>
<p>
  Typing <code>.</code> after an expression, or an identifier, offers member completions. Press
  <kbd>Ctrl</kbd> + <kbd>Space</kbd> to request them explicitly. Completions come from the project
  index and appear once it is warm. Edits re-index in the background as you type, so completion and
  go-to-definition track your changes without reopening the project.
</p>

<h2>Find</h2>
<p>
  <kbd>Ctrl</kbd> + <kbd>F</kbd> searches the current file. <kbd>Ctrl</kbd> + <kbd>Shift</kbd> +
  <kbd>F</kbd> opens <strong>Find in project</strong> — a backend-powered search across the whole
  project with <strong>Match case</strong>, <strong>Whole word</strong> and <strong>Regex</strong>
  toggles, grouping hits by file with the match highlighted; ↑/↓ move the selection and
  <kbd>Enter</kbd> opens the hit. If a word is <strong>selected</strong> in the editor, it
  pre-fills the search field (both here and in Find-in-file).
</p>

<h2>Go to class / file</h2>
<p>
  <kbd>Ctrl</kbd> + <kbd>N</kbd> opens <strong>Go to Class</strong> and
  <kbd>Ctrl</kbd> + <kbd>Shift</kbd> + <kbd>N</kbd> opens <strong>Go to File</strong> — a filterable
  quick-open. Type part of a name, ↑/↓ to move, <kbd>Enter</kbd> to open; a class jumps straight to
  its declaration line. A word selected in the editor pre-fills the filter.
</p>

<h2>Mojibake check</h2>
<p>
  <strong>Check file for mojibake</strong> (Command Palette) scans the open file for text that was
  UTF-8 but got read as Windows-1252 — the classic <code>Ã©</code> for <code>é</code> or
  <code>â€™</code> for <code>'</code>. Each hit is squiggled with a one-click
  <strong>Replace with «…»</strong> quick-fix, and a summary tells you how many were found.
  Detection is exact (a table of real corruption sequences), so clean accented text is never flagged.
</p>

<h2>Find usages</h2>
<p>
  Put the caret on a class, method or field and press <kbd>Alt</kbd> + <kbd>F7</kbd> to list every
  place it's used across the project in a popover — pick one to jump to it. It answers once the index
  is warm.
</p>

<h2>Save</h2>
<p>
  <kbd>Ctrl</kbd> + <kbd>S</kbd> writes the current file to disk in the project's encoding. Rename
  applies and saves its edits the same way.
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

<h2>Hover</h2>
<p>
  Rest the pointer on a class, method or field to see a card with its signature, declaring type and
  Javadoc (when the source has one). It answers from the project index, so it appears once the index
  is warm.
</p>

<h2>Right-click menu</h2>
<p>
  Right-clicking in the editor opens a context menu with the clipboard actions (Cut · Copy · Paste)
  and the semantic ones — <strong>Go to declaration</strong>, <strong>Find usages</strong>,
  <strong>Rename</strong>, <strong>Generate</strong> and <strong>Save</strong>. The semantic actions
  act on the symbol <strong>under the pointer</strong> — right-clicking moves the caret there first.
</p>

<h2>Intentions</h2>
<p>
  <kbd>Alt</kbd> + <kbd>Enter</kbd> opens the <strong>intentions</strong> popup at the caret — a
  keyboard-driven list of the context actions available there (↑/↓ to move, <kbd>Enter</kbd> to
  apply, <kbd>Esc</kbd> to dismiss). It's the entry point to the generator flows and, as the
  language service grows, to quick-fixes like adding a missing import or surrounding a block.
</p>
<p>
  Two quick-fixes already live here. With the caret inside a logging call whose message is built by
  string concatenation — <code>logger.info("user " + id + " logged in")</code> — the popup offers
  <strong>Replace concatenation with parameterized logging</strong>, rewriting it to the form the
  logging APIs prefer: <code>logger.info("user &lbrace;&rbrace; logged in", id)</code> (a trailing
  exception argument is kept last). On a <code>x.equals("literal")</code> call it offers
  <strong>Flip to null-safe equals</strong> — <code>"literal".equals(x)</code>, which never throws
  when <code>x</code> is null. And a family of one-click <strong>simplifications</strong>:
  <code>list.size() == 0</code> → <code>list.isEmpty()</code>, <code>flag == true</code> →
  <code>flag</code>, <code>!(a == b)</code> → <code>a != b</code>.
</p>

<h2>Rename</h2>
<p>
  Put the caret on a symbol and press <kbd>Shift</kbd> + <kbd>F6</kbd> to rename it across the
  project. A <strong>preview</strong> lists every edit grouped by file before anything is written —
  confirm to apply (through the editor, so a single <kbd>Ctrl</kbd> + <kbd>Z</kbd> undoes the whole
  rename). What gets rewritten depends on what the caret is on:
</p>
<ul>
  <li>a <strong>local variable</strong> or <strong>parameter</strong> — scope-exact, in that method only, never a same-named variable elsewhere or a field of the same name;</li>
  <li>a <strong>method</strong> or <strong>field</strong> — its declaration and every use across the project;</li>
  <li>a <strong>class</strong> or <strong>interface</strong> — its declaration, references, <code>import</code> statements, and the matching Spring <code>&lt;bean class="…"&gt;</code> entries. A Struts <code>&lt;action class="…"&gt;</code> names a bean id, not the class, so it is left untouched.</li>
</ul>
<p>
  Edits that can't be pinned down exactly — an overloaded method's call sites, for instance — are
  marked for review in the preview rather than applied silently. It answers once the index is warm.
  OGNL and JSP references are not rewritten yet.
</p>

<h2>Spelling</h2>
<p>
  Opt-in per project (Project Configuration → <strong>Spelling</strong>): after downloading the
  English + Italian dictionaries, Bennu checks your <strong>declared names</strong> — split by
  camelCase, snake_case and kebab-case — and your <strong>comments</strong>. A misspelled word is
  underlined as a hint; <kbd>Alt</kbd> + <kbd>Enter</kbd> (or the lint action) offers to replace it
  with a suggestion or <strong>add it to a project or global dictionary</strong>. Common programming
  abbreviations are allow-listed, so it stays quiet on the usual jargon.
</p>

<h2>Generate</h2>
<p>
  <kbd>Alt</kbd> + <kbd>Insert</kbd> opens the <strong>Generate</strong> dialog — build a constructor,
  getters, setters or both from the active class's fields. Pick a mode, tick the fields to include,
  choose fluent or plain setters and camelCase or snake_case accessors; a live preview shows the code
  and <kbd>Ctrl</kbd> + <kbd>Enter</kbd> inserts it at the caret.
</p>

<h2>Indentation</h2>
<p>
  The footer shows the active indentation as <em>Spaces: N</em> or <em>Tab Size: N</em>. Click it (or
  focus it and press <kbd>↑</kbd>/<kbd>↓</kbd>) to switch between <strong>spaces and tabs</strong> and
  pick the <strong>tab width</strong> (2 / 4 / 8). The change applies to the open editor immediately.
</p>

<h2>JSP &amp; Struts navigation</h2>
<p>
  JSP files are highlighted by a dedicated grammar — namespaced taglib tags (<code>&lt;s:iterator&gt;</code>,
  <code>&lt;c:if&gt;</code>), scriptlets, EL <code>$&lbrace;…&rbrace;</code> and OGNL <code>%&lbrace;…&rbrace;</code> all colour
  correctly, and the <strong>inside</strong> of an EL/OGNL expression is tokenized too —
  identifiers, property accesses, strings, numbers, operators and keywords each get their own
  colour instead of one flat block. <kbd>Ctrl</kbd> + <kbd>B</kbd> (or <kbd>Ctrl</kbd> + click) on an
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

<h2>MyBatis mappers</h2>
<p>
  Inside a mapper <code>.xml</code>, <kbd>Ctrl</kbd> + <kbd>B</kbd> (or <kbd>Ctrl</kbd> + click) follows
  the piece under the caret: a statement <code>id="…"</code> jumps to the matching method on the
  mapper interface (<code>.java</code>); the mapper <code>namespace="…"</code> opens that interface;
  an <code>&lt;include refid="…"&gt;</code> jumps to the <code>&lt;sql&gt;</code> fragment it pulls in;
  and a statement's <code>resultMap="…"</code> jumps to the <code>&lt;resultMap&gt;</code> it uses.
  Fragment references within the same file resolve instantly (no index needed).
</p>

<h2>Struts validation files</h2>
<p>
  From a Java <strong>action class</strong> the toolbar shows a <strong>Validation</strong> button
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

<h2>The index</h2>
<p>
  Completion, go-to-definition, find-usages, rename, hover and Go-to-Class all answer from a
  <strong>semantic index</strong> Bennu builds in the background when a project opens. The footer shows
  its progress and reads <em>Indexed · N types</em> once it's warm.
</p>
<p>
  The <strong>Index inspector</strong> (Command Palette → <em>Index inspector…</em>) browses what the
  index holds — types, members, jars, JDK, beans, actions and relations — with a filter and jump-to.
  If something looks stale or a class you know exists isn't turning up, press <strong>Rebuild</strong>
  there (or run <em>Rebuild index</em> from the palette) to invalidate the index and recompute it from
  scratch. This is a pure re-scan of the sources on disk — it doesn't compile the project (that's
  <kbd>Ctrl</kbd> + <kbd>F9</kbd>).
</p>

<div class="callout">
  Everything is reachable from the keyboard. The <strong>Command Palette</strong>
  (<kbd>Ctrl</kbd> + <kbd>K</kbd>) lists the editor and tool-window actions; the tool windows toggle
  with <kbd>Alt</kbd> + <kbd>1</kbd> / <kbd>2</kbd> (Project · Structure), <kbd>Alt</kbd> +
  <kbd>0</kbd> / <kbd>6</kbd> / <kbd>7</kbd> / <kbd>F12</kbd> (Build · Problems · TODO · Terminal), and
  <kbd>Alt</kbd> + <kbd>8</kbd> / <kbd>9</kbd> (Maven · Services). Build the project with
  <kbd>Ctrl</kbd> + <kbd>F9</kbd> and run it with <kbd>Shift</kbd> + <kbd>F10</kbd>.
</div>
