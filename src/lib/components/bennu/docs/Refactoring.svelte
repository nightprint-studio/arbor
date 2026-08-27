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
<h2>Quick-fixes</h2>
<p>
  With the caret on a <strong>diagnostic</strong>, <kbd>Alt</kbd> + <kbd>Enter</kbd> offers its
  repair — not just the sentence saying what is wrong:
</p>
<ul>
  <li><strong>Unused, duplicate or redundant import</strong> → remove it, with its line.</li>
  <li><strong>Unhandled checked exception</strong> → two ways out: add <code>throws</code> to the
    enclosing method (extending the clause it already has, if any), or surround the statement with a
    <code>try</code>/<code>catch</code>.</li>
  <li><strong>Non-exhaustive enum switch</strong> → write the missing cases, in the form the switch
    already uses (arrows or colons, never a mix — that doesn't compile).</li>
  <li><strong>Comparing strings with <code>==</code></strong> → <code>equals</code>, with the literal
    moved to the receiver side so it cannot throw. <code>!=</code> keeps its negation.</li>
  <li><strong>Switch fall-through</strong> → add the missing <code>break;</code>, indented with the
    group it ends.</li>
  <li><strong>A stray <code>;</code></strong> → remove it.</li>
  <li><strong>A missing import</strong> → add it, one entry per candidate package (see above).</li>
</ul>
<p>
  A fix is keyed to the <em>kind</em> of diagnostic and reads the source itself, so it is never
  guessing from the wording. The two that need to know types — which exception, which constants —
  are recomputed from the same analysis that raised the diagnostic, so a fix that appears is one
  that will actually clear the squiggle.
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
<h2>Implement / override methods</h2>
<p>
  With the caret inside a class, <kbd>Alt</kbd> + <kbd>Enter</kbd> →
  <strong>Implement / override methods…</strong> (also in the Command Palette) lists everything the
  class inherits and is allowed to override, <strong>grouped by the type that declares it</strong>.
  Tick the ones you want — a group's own box takes all of them at once — and
  <kbd>Ctrl</kbd> + <kbd>Enter</kbd> writes them just inside the class's closing brace, as a single
  undo step.
</p>
<p>
  <strong>Abstract methods are ticked when the dialog opens</strong>, and nothing else is: those are
  the ones the compiler will demand, so implementing an interface is one gesture, while overriding
  something that already works stays a decision. An abstract method's body throws
  <code>UnsupportedOperationException</code> — a stub that returned <code>null</code> would compile,
  run, and lie. A concrete one's body starts with <code>super.…</code>, because overriding one
  usually means adding to it.
</p>
<p>
  Only what Java would actually let you override is offered: never a <code>static</code>,
  <code>final</code> or <code>private</code> method, never a constructor, never a package-private
  method from another package, and never one this class already declares — matched on the parameter
  types, so the overloads you have not written yet are still there. The types the new methods
  mention are <strong>imported in the same step</strong>; generated code that does not compile is
  not generated code.
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
