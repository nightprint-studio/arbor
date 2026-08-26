<!-- Bennu docs — changing code rather than reading it: intentions, rename, generate. -->
<h1>Refactoring &amp; intentions</h1>
<p class="doc-lead">
  The edits Bennu makes for you. All of them go through the same reference index the navigation
  does, which is what separates a rename from a search-and-replace: the index knows which
  <code>getName</code> is <em>this</em> one.
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
  apply, <kbd>Esc</kbd> to dismiss). It's the entry point to the generator flows and to quick-fixes.
</p>
<p>
  With the caret on a <strong>type that isn't imported</strong>, the popup offers
  <strong>Import '…'</strong> — it adds the <code>import</code> line for you (placed after the package
  declaration and sorted among the existing imports). When more than one class shares that name, each
  candidate is listed as its own entry, so you pick the package you meant; a type in the same package,
  in <code>java.lang</code>, or already covered by a wildcard import isn't offered (it needs none).
</p>
<p>
  More quick-fixes live here too. With the caret inside a logging call whose message is built by
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
  <li>a <strong>method</strong> or <strong>field</strong> — its declaration and every use across the project, including uses whose receiver is only typed through a library generic (a lambda parameter off <code>list.stream().map(…)</code>). A method carries its whole <strong>override family</strong> with it: the abstract or interface declaration it comes from, and every implementation that overrides it, since to a caller those are one method. All <strong>overloads</strong> of the name move together for the same reason;</li>
  <li>a <strong>record component</strong>, or a field whose accessors <strong>Lombok</strong> generates — the field itself, plus the call sites of accessors nobody wrote down. <code>failure.sourcePath()</code> and <code>order.getCustomerName()</code> appear at every caller even though the methods appear nowhere, so they move with the field; getters, setters and <code>@With</code> copy-methods all follow. An accessor you wrote by hand is a declaration in its own right and is left alone;</li>
  <li>a <strong>class</strong> or <strong>interface</strong> — its declaration, references, <code>import</code> statements, and the matching Spring <code>&lt;bean class="…"&gt;</code> entries. A Struts <code>&lt;action class="…"&gt;</code> names a bean id, not the class, so it is left untouched. When the file is named after the type — a public top-level one — <strong>the file is renamed with it</strong>, since Java requires the two to match; a nested type's file is named after its outer type and stays where it is.</li>
</ul>
<p>
  Edits that can't be pinned down exactly — an overloaded method's call sites, for instance — are
  marked for review in the preview rather than applied silently. It answers once the index is warm.
  OGNL and JSP references are not rewritten yet.
</p>
<p>
  A member reached through an <code>import static</code> carries both the import and the bare calls
  with it, and so does a <strong>method reference</strong> — <code>Failure::sourcePath</code> moves
  with the method it names. And a rename is <strong>refused</strong> when it would break an override of code that
  can't follow: a method implementing an interface from a dependency has its name fixed by that
  dependency, so renaming only your side leaves a class that no longer implements what it declares.
  The preview still shows what it would have done, and names the library type, but won't apply it.
</p>
<h2>Generate</h2>
<p>
  <kbd>Alt</kbd> + <kbd>Insert</kbd> opens the <strong>Generate</strong> dialog — build a constructor,
  getters, setters or both from the active class's fields. Pick a mode, tick the fields to include,
  choose fluent or plain setters and camelCase or snake_case accessors; a live preview shows the code
  and <kbd>Ctrl</kbd> + <kbd>Enter</kbd> inserts it at the caret.
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
