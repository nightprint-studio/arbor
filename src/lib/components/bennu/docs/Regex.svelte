<script lang="ts">
  /**
   * Regular expressions in the editor: the languages a string literal can hold (regex first among them), how a
   * regex is recognised, how it is coloured and checked, and the Regex Tester.
   */
  import Callout from '$lib/components/shared/ui/Callout.svelte';
  import { highlightCode } from '$lib/utils/highlight';
</script>

<span class="eyebrow">Editor</span>
<h1>Regex &amp; languages in strings</h1>

<p class="doc-lead">
  A string literal is often another language written inside Java. Bennu colours it as that language — a regex above all —
  and the <strong>Regex Tester</strong> lets you try a pattern against real text and write it back into the literal with the
  right escaping.
</p>

<h2>Languages in strings</h2>
<p>
  Mark a literal the way IntelliJ does and it is coloured as the language you name, with the Java escapes
  (<code>\n</code>, <code>\"</code>, <code>\\</code>, text-block indentation) undone first — so what is coloured is the text
  the program actually sees, drawn over the characters that wrote it.
</p>
<pre class="doc-code">{@html highlightCode(`// language=lua
String script = "local x = 1\\nprint(x)";

String page = /* language=html */ "<p class=\\"a\\">hi</p>";

@Language("css") String style = "a { color: red }";`, 'java')}</pre>
<table>
  <thead><tr><th>Name after <code>language=</code></th><th>Coloured as</th></tr></thead>
  <tbody>
    <tr><td><code>regex</code>, <code>RegExp</code></td><td>Regular expression (see below)</td></tr>
    <tr><td><code>lua</code></td><td>Lua</td></tr>
    <tr><td><code>html</code></td><td>HTML</td></tr>
    <tr><td><code>js</code>, <code>javascript</code></td><td>JavaScript</td></tr>
    <tr><td><code>css</code></td><td>CSS</td></tr>
    <tr><td><code>json</code></td><td>JSON</td></tr>
    <tr><td><code>xml</code></td><td>XML</td></tr>
    <tr><td><code>yaml</code></td><td>YAML</td></tr>
    <tr><td><code>sh</code>, <code>shell</code>, <code>bash</code></td><td>Shell</td></tr>
  </tbody>
</table>
<p>
  The marker applies to the first literal after it, and a <code>"a" + "b"</code> concatenation counts as one. The same
  comments work in JavaScript, TypeScript and Rust files. <strong>SQL</strong> is handled by the JPA support, which also
  completes and checks it, so it is not part of this list.
</p>

<h2>Regex, recognised without a marker</h2>
<p>
  A regex needs no comment when the code already says what it is. These are coloured as patterns on their own:
</p>
<ul>
  <li>Java: the pattern argument of <code>Pattern.compile</code>, <code>Pattern.matches</code>, <code>String.matches</code>,
    <code>replaceAll</code>, <code>replaceFirst</code> and <code>split</code> (when it contains a regex character),
    <code>Scanner.useDelimiter</code>, <code>findInLine</code>, and the <code>regexp</code> attribute of
    <code>@Pattern</code> and <code>@Email</code>.</li>
  <li>JavaScript / TypeScript: <code>new RegExp("…")</code>. Rust: <code>Regex::new(…)</code> and
    <code>RegexBuilder::new(…)</code>, including raw strings.</li>
</ul>
<p>
  A replacement (<code>matcher.replaceAll("$1")</code>) and the input of a splitter are deliberately left alone.
  <strong>Settings › Editor › Languages in strings</strong> chooses how eager this is:
</p>
<ul>
  <li><strong>By context</strong> (default) — the list above.</li>
  <li><strong>By context and variable names</strong> — also a literal assigned to a variable named like a pattern
    (<code>emailRegex</code>, <code>PHONE_PATTERN</code>). A guess, so opt-in; date formats such as
    <code>"dd/MM/yyyy"</code> are excluded.</li>
  <li><strong>Only when marked</strong> — nothing but <code>// language=regex</code>.</li>
</ul>

<h2>How a regex is coloured and checked</h2>
<p>
  Groups and look-arounds read as keywords, quantifiers and <code>|</code> as operators, <code>\d</code>-style classes and
  <code>.</code> as types, anchors as constants, back-references and group names as fields, escapes in the escape colour.
  The pattern is read under the rules of its language — Java in Java files, JavaScript and Rust in theirs — so
  <code>a*+</code> is fine in Java and wavy-underlined in a <code>new RegExp</code>.
</p>
<p>
  A mistake is underlined where it is, with the message Java itself would give: an unclosed group or class, an unmatched
  <code>)</code>, a dangling <code>*</code>, an illegal range, an unknown <code>\p&#123;…&#125;</code> property, a look-behind
  without a maximum length, a back-reference to a group that does not exist. A pattern built with <code>+</code> from
  something Bennu cannot see is not checked for balance, since half a pattern proves nothing.
</p>

<h2>The Regex Tester</h2>
<p>
  <kbd>Ctrl</kbd> + <kbd>Shift</kbd> + <kbd>X</kbd>, <strong>Regex Tester…</strong> in the command palette, or
  <strong>Open in Regex Tester…</strong> in the context menu of a string literal. With the caret in a literal the tester opens
  on its <em>value</em>; with a selection outside one it opens on the selected text; otherwise it opens empty.
</p>
<div class="feature-grid two-col">
  <div class="feature-card">
    <div class="fc-eyebrow">Pattern</div>
    <div class="fc-title">Flavour and flags</div>
    <div class="fc-desc">Java, JavaScript or Rust / PCRE syntax; the flags of that flavour as keys; find-all, whole-text
      or start-only matching.</div>
  </div>
  <div class="feature-card">
    <div class="fc-eyebrow">Test text</div>
    <div class="fc-title">Live matches</div>
    <div class="fc-desc">Every match is painted in the text, with the innermost group underlined; the Matches tab lists
      them with indexes, names and spans.</div>
  </div>
  <div class="feature-card">
    <div class="fc-eyebrow">Substitution</div>
    <div class="fc-title">Replace and split</div>
    <div class="fc-desc">The result of <code>replaceAll</code> with your template (<code>$1</code>, <code>&#36;&#123;name&#125;</code>
      for Java and Rust, <code>$&lt;name&gt;</code> for JavaScript) and of <code>String.split</code>.</div>
  </div>
  <div class="feature-card">
    <div class="fc-eyebrow">Explain · Insert · Saved</div>
    <div class="fc-title">Reading and keeping</div>
    <div class="fc-desc">The pattern explained token by token (hover a row and its token lights up in the field), an <strong>Insert</strong> menu with the syntax reference, and named and recent patterns kept
      with the text they were tried on.</div>
  </div>
</div>
<p>
  <strong>Copy as…</strong> gives the pattern as a Java string, a <code>Pattern.compile</code> call with its flags, a
  JavaScript literal or a Rust <code>Regex::new</code>. When the tester was opened from a literal,
  <strong>Apply to source</strong> (<kbd>Ctrl</kbd> + <kbd>Enter</kbd>) writes the edited pattern back with that literal's own
  escaping: doubled backslashes in a string, the indentation kept in a text block, no escaping in a Rust raw string, and a
  concatenation collapsed into one literal.
</p>
<Callout variant="info" title="A pattern that never finishes cannot freeze the window">
  Every run happens in a separate worker with a two-second limit. A catastrophic pattern such as
  <code>(a+)+$</code> against a long run of <code>a</code>s is stopped and reported as <strong>Timed out</strong>; typing
  again starts a fresh run.
</Callout>
<Callout variant="warning" title="Which engine runs the pattern">
  Java and Rust / PCRE patterns run on Arbor's own matcher, written to follow <code>java.util.regex</code> — greedy, lazy and
  possessive quantifiers, atomic groups, look-around, back-references, inline flags, Java's line-terminator rules. No JVM is
  started. JavaScript patterns run on the browser engine itself. For the Rust / PCRE flavour the syntax rules are the
  flavour's, but the matching is the same backtracking engine, so a corner where the Rust engine differs (Unicode
  classes, for one) is not reproduced.
</Callout>
