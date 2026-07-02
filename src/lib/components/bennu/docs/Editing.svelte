<!-- Bennu docs — Editing & navigation. -->
<h1>Editing &amp; navigation</h1>
<p class="doc-lead">
  The editor is a fast, syntax-highlighted view of your Java (and JSP) files with a tab strip,
  a symbol structure, code folding, completions, project-wide search and keyboard-first navigation.
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
  editor to its declaration.
</p>

<h2>Project tree</h2>
<p>
  The <strong>Project</strong> panel header carries quick actions: create a new file, locate the open
  file in the tree, collapse or expand the whole tree, and an options menu.
</p>

<h2>Code folding</h2>
<p>
  Braced blocks (classes, methods, blocks) and block comments fold from the gutter chevrons; the head
  line stays visible. Folding is computed live from the syntax tree — no indexing needed.
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
  <kbd>F</kbd> opens <strong>Find in project</strong> — a modal listing every matching line across the
  project; ↑/↓ move the selection and <kbd>Enter</kbd> opens the hit.
</p>

<h2>Go to line</h2>
<p>
  <kbd>Ctrl</kbd> + <kbd>G</kbd> opens the go-to-line box — type <code>42</code> or
  <code>42:8</code> (line:column) and press <kbd>Enter</kbd>.
</p>

<h2>Go to definition</h2>
<p>
  Put the caret on a JSP form or link <strong>action reference</strong> — an
  <code>action="…"</code> value or a path like <code>/do/Category/viewTree</code> — and press
  <kbd>Ctrl</kbd> + <kbd>B</kbd> (or <kbd>Ctrl</kbd> + click) to jump to where the action is
  declared: the Struts config fragment, or its view JSP. If the reference resolves only to an
  implementation class the class name is shown. It relies on the project's config graph, so it
  answers once the index is warm and stays quiet when a reference can't be resolved.
</p>

<h2>Intentions</h2>
<p>
  <kbd>Alt</kbd> + <kbd>Enter</kbd> opens the <strong>intentions</strong> popup at the caret — a
  keyboard-driven list of the context actions available there (↑/↓ to move, <kbd>Enter</kbd> to
  apply, <kbd>Esc</kbd> to dismiss). It's the entry point to the generator flows and, as the
  language service grows, to quick-fixes like adding a missing import or surrounding a block.
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

<h2>Generate</h2>
<p>
  <kbd>Alt</kbd> + <kbd>Insert</kbd> opens the <strong>Generate</strong> dialog — build a constructor,
  getters, setters or both from the active class's fields. Pick a mode, tick the fields to include,
  choose fluent or plain setters and camelCase or snake_case accessors; a live preview shows the code
  and <kbd>Ctrl</kbd> + <kbd>Enter</kbd> inserts it at the caret.
</p>

<div class="callout">
  Everything is reachable from the keyboard. The <strong>Command Palette</strong>
  (<kbd>Ctrl</kbd> + <kbd>K</kbd>) lists the editor and tool-window actions; the tool windows toggle
  with <kbd>Alt</kbd> + <kbd>1</kbd> / <kbd>2</kbd> (Project · Structure), <kbd>Alt</kbd> +
  <kbd>6</kbd> / <kbd>Alt</kbd> + <kbd>F12</kbd> (Problems · Terminal), and <kbd>Alt</kbd> +
  <kbd>8</kbd> / <kbd>9</kbd> (Maven · Services).
</div>
