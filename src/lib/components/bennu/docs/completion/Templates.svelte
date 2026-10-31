<script lang="ts">
  /** Completion — annotations, abbreviations and postfix. */
  import Callout from '$lib/components/shared/ui/Callout.svelte';
  import { highlightCode } from '$lib/utils/highlight';
</script>

<span class="eyebrow">Completion</span>
<h1>Annotations, abbreviations and postfix</h1>

<p class="doc-lead">
  Names that expand into code: annotations, your own abbreviations, and postfix templates.
</p>

<h2>Annotations</h2>
<p>
  Typing <strong><code>@</code></strong> opens the popup on its own, with <strong>annotation types only</strong> — from every type on the classpath to
  the few hundred annotations on it, so the list is short and nearly always holds the answer.
</p>
<ul>
  <li>It narrows again from where the caret is: above a field, one whose <code>@Target</code> only allows a method sorts below the ones that belong
    there. Ranked, not hidden — an annotation from unread bytes reports no target, and reading that as a refusal would hide it.</li>
  <li>The list is complete from the <strong>first letter</strong>: what is capped is how many annotations are offered, not how many names are looked
    at, so <code>@S</code> reaches <code>SuppressWarnings</code> past the hundreds of classes also starting with <code>S</code>.</li>
  <li>Accepting one adds its import, and the ones you accept come first for as long as the project stays open — never written to disk, never above an
    import the file already made.</li>
</ul>

<h2>Abbreviations</h2>
<p>
  Type <code>psf</code> and the list offers <strong>public static final</strong>, with <code>psfi</code> and <code>psfs</code> under it for
  <code>int</code> and <code>String</code>. These are IntelliJ's own Java abbreviations, deliberately — fifteen years of <code>psf</code> should work
  here without configuring anything.
</p>
<table>
  <thead><tr><th>Family</th><th>Abbreviations</th></tr></thead>
  <tbody>
    <tr><td>Modifiers</td><td><code>pf</code>, <code>prf</code>, <code>psf</code>, <code>psfi</code>,<code>psfs</code>, <code>prsf</code>, <code>prsfi</code>, <code>prsfs</code>, <code>psvm</code> for a <code>main</code></td></tr>
    <tr><td>Printing</td><td><code>sout</code>, <code>souf</code>, <code>serr</code></td></tr>
    <tr><td>Statements</td><td><code>fori</code>, <code>ifn</code>, <code>inn</code>, <code>thr</code>, <code>iter</code></td></tr>
  </tbody>
</table>
<ul>
  <li>Several blanks are walked with <kbd>Tab</kbd>, and the body is <strong>re-indented</strong> to where it lands — a <code>psvm</code> three levels
    into a class arrives lined up.</li>
  <li>They are <em>added</em> to what the index found, never substituted: a field called <code>psfCount</code> still appears, under them.</li>
  <li>Offered only where a bare word is typed — after a <code>.</code> the list answers "what members does this have".</li>
  <li>Abbreviations of your own sit beside them, written as code templates — see <strong>Code templates</strong>.
    Yours are not Java's: the language in a template's name says where it is offered, so a <code>dbg.rs.jinja</code>
    expands in a Rust file, answered by rust-analyzer, and one written with no language expands in any file. The
    built-in family above stays Java's.</li>
</ul>
<p>
  <code>iter</code> is the one that <em>reads the surrounding code</em>: it loops over the nearest array or <code>Iterable</code> local or parameter
  in scope, with the element type and a name spelled the project's way (<code>var</code> from Java 10). It is offered only where such a variable
  exists, at the start of a statement, and its row says which variable it loops over.
</p>
<Callout variant="info" title="What is deliberately missing">
  Abbreviations that would have to <em>guess</em> — a version that guesses writes a name that is not there. <code>iter</code> is offered only when
  there is something to iterate.
</Callout>

<h2>Postfix templates</h2>
<p>Write the expression first, then say what to do with it:</p>
<pre><code>{@html highlightCode(`order.getTotal().nn`, 'java')}</code></pre>
<pre><code>{@html highlightCode(`if (order.getTotal() != null) {
    |
}`, 'java')}</code></pre>
<p>
  The point is not keystrokes: it lets you think in the order you think — the value, then the control flow around it — instead of committing to an
  <code>if (</code> before you know what goes in it. They appear in the completion list under the name you type, with IntelliJ's names on purpose.
</p>
<p>
  What a template writes depends on the <strong>type</strong> of the expression, and a template is only listed where it applies:
  <code>.for</code> on a collection or an array, <code>.if</code> on a boolean, <code>.nn</code> on a value that can be <code>null</code>,
  <code>.ifpe</code> on an <code>Optional</code>. The names it proposes come from the code, spelled in the project's naming convention for local variables or, where none is set, in the one the
  file's names follow — <code>orders.for</code> declares an <code>order</code>
  of the element type — and the imports its text needs are added with it. Templates appear once the type is known and you have started typing a name
  after the dot.
</p>
<table>
  <thead><tr><th>Family</th><th>Templates</th></tr></thead>
  <tbody>
    <tr><td>Null checks</td><td><code>.nn</code> / <code>.notnull</code>, <code>.null</code>, <code>.reqnonnull</code></td></tr>
    <tr><td>Conditions</td><td><code>.if</code>, <code>.else</code>, <code>.while</code>, <code>.not</code>, <code>.assert</code>, <code>.switch</code></td></tr>
    <tr><td>Loops</td><td><code>.for</code> / <code>.iter</code>, <code>.fori</code>, <code>.forr</code>, <code>.forEach</code>, <code>.stream</code></td></tr>
    <tr><td>Optional</td><td><code>.opt</code>, <code>.optof</code>, <code>.ifp</code>, <code>.ifpe</code>, <code>.ifget</code>, <code>.orThrow</code>, <code>.orGet</code></td></tr>
    <tr><td>Statements</td><td><code>.var</code>, <code>.return</code>, <code>.throw</code>, <code>.try</code>, <code>.twr</code>, <code>.synchronized</code>, <code>.sout</code>, <code>.soutv</code>, <code>.souf</code>, <code>.serr</code></td></tr>
    <tr><td>Expressions</td><td><code>.inst</code> / <code>.instanceof</code>, <code>.cast</code>, <code>.castvar</code>, <code>.par</code>, <code>.arg</code>, <code>.lambda</code>, <code>.format</code>, <code>.new</code> on a class name</td></tr>
  </tbody>
</table>
<p>
  They follow the <strong>Java level</strong> of the file's module, and write what that level compiles rather than disappearing:
</p>
<table>
  <thead><tr><th>Template</th><th>Below</th><th>From</th></tr></thead>
  <tbody>
    <tr><td><code>.var</code>, <code>.for</code>, <code>.twr</code></td><td>the declared type, <code>Order order</code></td><td>Java 10: <code>var order</code></td></tr>
    <tr><td><code>.inst</code></td><td><code>instanceof</code> and a cast into a local</td><td>Java 16: a type pattern</td></tr>
    <tr><td><code>.ifpe</code></td><td><code>if (…isPresent()) … else …</code></td><td>Java 9: <code>ifPresentOrElse</code></td></tr>
    <tr><td><code>.stream</code> on an <code>Optional</code></td><td><code>map(Stream::of).orElseGet(Stream::empty)</code></td><td>Java 9: not listed, <code>stream()</code> is its member</td></tr>
  </tbody>
</table>
<p>
  An expression that does work is never written twice: where the older form needs the value in two places, <code>repo.findById(id).ifget</code>
  declares a local for it first, and <code>.fori</code>, whose bound runs on every iteration, is not offered on a call. Templates with no equivalent
  at the level — <code>Optional</code> before Java 8, <code>.twr</code> before 7 — are not listed. Rust gets its own postfix set from rust-analyzer, in
  the same list.
</p>
<p>
  Postfix templates of your own sit beside them — a template of the <strong>Postfix</strong> kind, offered on the values it says it applies to. See
  <strong>Code templates</strong>.
</p>
