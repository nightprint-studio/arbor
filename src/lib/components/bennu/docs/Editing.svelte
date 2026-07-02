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
  index and appear once it is warm.
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

<div class="callout">
  Everything is reachable from the keyboard. The <strong>Command Palette</strong>
  (<kbd>Ctrl</kbd> + <kbd>K</kbd>) lists the editor and tool-window actions; the tool windows toggle
  with <kbd>Alt</kbd> + <kbd>1</kbd> / <kbd>2</kbd> (Project · Structure), <kbd>Alt</kbd> +
  <kbd>6</kbd> / <kbd>Alt</kbd> + <kbd>F12</kbd> (Problems · Terminal), and <kbd>Alt</kbd> +
  <kbd>8</kbd> / <kbd>9</kbd> (Maven · Services).
</div>
