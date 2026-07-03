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
  editor to its declaration. The header carries <strong>Collapse all</strong> / <strong>Expand
  all</strong> chevrons to fold or unfold the whole tree at once, just like the Project panel.
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
  <kbd>Enter</kbd> opens the hit.
</p>

<h2>Go to class / file</h2>
<p>
  <kbd>Ctrl</kbd> + <kbd>N</kbd> opens <strong>Go to Class</strong> and
  <kbd>Ctrl</kbd> + <kbd>Shift</kbd> + <kbd>N</kbd> opens <strong>Go to File</strong> — a filterable
  quick-open. Type part of a name, ↑/↓ to move, <kbd>Enter</kbd> to open; a class jumps straight to
  its declaration line.
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
