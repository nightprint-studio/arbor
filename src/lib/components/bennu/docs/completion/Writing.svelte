<script lang="ts">
  /** Completion — matching and writing. */
  import Callout from '$lib/components/shared/ui/Callout.svelte';
  import { highlightCode } from '$lib/utils/highlight';
</script>

<span class="eyebrow">Completion</span>
<h1>Matching and writing</h1>

<p class="doc-lead">
  How the letters you type are matched, what accepting a row writes, and how to read a row.
</p>

<h2>How a name is matched</h2>
<p>
  <strong>A name does not have to be spelled the way it is declared.</strong> What you type is matched three ways, best first, and which one
  matched is part of the order:
</p>
<table>
  <thead><tr><th>Match</th><th>Typed</th><th>Reaches</th></tr></thead>
  <tbody>
    <tr><td>An exact prefix</td><td><code>toLo</code></td><td><code>toLowerCase</code></td></tr>
    <tr><td>A prefix ignoring case</td><td><code>TOLO</code></td><td><code>toLowerCase</code> — a shift held one letter too long</td></tr>
    <tr><td>The camel humps</td><td><code>tolc</code>, <code>aAE</code>, <code>SBA</code>, <code>MAXV</code></td><td><code>toLowerCase</code>, <code>addAllElements</code>, <code>SpringBootApplication</code>, <code>MAX_VALUE</code> — an underscore starts a word too</td></tr>
  </tbody>
</table>
<p>
  It is <strong>one rule for every kind of candidate</strong> — a member, a local, a static import, a class name.
</p>
<p>
  <strong>Settings → Completion → Popup</strong> decides when the popup arrives and how forgiving it is: whether it opens on its own while you
  type, after how long a pause, or only on the chord; and <em>case-sensitive matching</em>. That last is about case, not humps — off, only the
  first letter must agree, the one that carries information in Java since a capital means a type; on, every letter must, so <code>aAE</code>
  still reaches <code>addAllElements</code> and <code>aae</code> no longer does.
</p>

<h2>A name with nothing to its left</h2>
<p>Java says exactly what a bare identifier can mean, and the popup offers those things nearest first:</p>
<ol class="step-list">
  <li><strong>What the scope binds</strong> — locals, method and lambda parameters, the <code>for</code> variable, a try-with-resources, a
    <code>catch</code> parameter, a pattern variable from <code>o instanceof Foo f</code>. A variable of this block beats a parameter of the
    method; in one block, the line above beats twenty lines up. A shadowing name is offered once, as the inner one — what the name resolves to.</li>
  <li><strong>The enclosing type's own members</strong>, and what it inherits, without <code>this.</code>.</li>
  <li><strong>What an <code>import static</code> brought in</strong>, with the type it came from on the row — the one case where nothing else
    on screen says where a bare name is from.</li>
  <li><strong>Class names</strong>, from the type-name index.</li>
</ol>
<p>
  A local is not in scope inside its own declaration, so <code>int counted = coun</code> offers <code>counter</code>, not <code>counted</code>.
  In a <code>static</code> method an instance member is not offered at all: it would not compile.
</p>
<Callout variant="info" title="Types or names first? Java's own convention decides">
  <code>Str</code> is a type being written and <code>str</code> a variable, and no score makes one the other. A capitalised name with a
  lowercase letter leads with types; anything else leads with what is in scope, where a <code>SCREAMING_CASE</code> constant of your own class
  lives. Both lists are always offered, so a prefix read the "wrong" way costs a scroll, not an answer.
</Callout>

<h2>What accepting one writes</h2>
<p>
  A method is a call: accepting <code>size</code> writes <code>size()</code>. When it takes arguments the caret lands <strong>between the
  parentheses</strong>, where the parameter hints strip is about to describe them; when it takes none, after them.
</p>
<ul>
  <li>Parameter <em>names</em> are not written — tabbing through <code>put(key, value)</code> placeholders to delete them is slower than typing,
    and a class file carries no names anyway.</li>
  <li>When the call is <strong>already written</strong> — <code>t.si()</code> with the caret after <code>si</code>, correcting a name — the
    parentheses are left alone, not doubled.</li>
  <li>A class picked <strong>after <code>new</code></strong> writes the call too: <code>ArrayList</code> becomes <code>ArrayList&lt;&gt;()</code> (the
    diamond only for a generic class), with the caret after it when a constructor takes nothing and between the parentheses when every one
    takes something. An <strong>interface or an abstract class</strong> writes <code>Runnable() &#123;&#125;</code> with the caret inside the braces — an anonymous class —
    except an abstract class that can only be built with arguments, where the caret waits between the parentheses. A name that stands for
    several classes (<code>Shape</code>, in <code>java.awt</code> and elsewhere) keeps just the name until you choose its import, and an enum is not a thing you create.</li>
  <li>An <strong>overloaded</strong> method is <em>one row per overload</em>, fewest parameters first — <code>wait()</code>,
    <code>wait(long timeoutMillis)</code>, <code>wait(long timeoutMillis, int nanos)</code> — since each leaves the caret in a different place.
    After <code>::</code> they are one row, because a method reference writes only the name. A method that merely <strong>overrides</strong> an
    inherited one appears once.</li>
  <li>What could not be written there is not offered: a <code>private</code> member of another class, a package-private one from another package,
    and a <code>protected</code> one through a receiver outside its package and your class — <code>route.clone()</code> and
    <code>route.finalize()</code> are not Java, <code>this.clone()</code> is.</li>
</ul>

<h2>Reading a row</h2>
<dl class="meta-grid">
  <dt>The icon</dt>
  <dd>What it <strong>is</strong>. A member is a circle — <code>m</code> method, <code>c</code> constructor, <code>f</code> field,
    <code>v</code> variable, <code>p</code> parameter — and a type a rounded square — <code>c</code> class, <code>i</code> interface,
    <code>e</code> enum, <code>r</code> record, <code>@</code> annotation; a package is a folder, a template <code>{'{}'}</code>. A dot in the
    corner marks <em>static</em>, a lock <em>final</em>, a dashed outline <em>abstract</em>, a diagonal stroke <em>deprecated</em>.</dd>
  <dt>The name</dt>
  <dd>What it is <strong>called</strong>, with the letters you typed marked. <strong>Bold</strong> when the receiver's own type declares it,
    regular when inherited, dimmed when it is <code>java.lang.Object</code>'s. A deprecated one is struck through.</dd>
  <dt>The parameters</dt>
  <dd>Beside the name, with their names where the member carries them: <code>equals(Object obj)</code>. A library compiled without
    parameter names shows the types alone.</dd>
  <dt>The right edge</dt>
  <dd>What a method <strong>returns</strong> or a field holds — <code>getClass()</code> as <code>Class&lt;? extends ServiceRoute&gt;</code>,
    typed after the receiver — and, for an inherited member only, where it <strong>comes from</strong>: what tells <code>List.of</code> from
    <code>Set.of</code>.</dd>
</dl>
<p>
  The <strong>documentation</strong> of the highlighted row appears beside the list — the card hover draws, from the same answer, so the two
  cannot describe one member two ways. It is fetched only for the row you highlight: a library's documentation is read from its sources archive,
  and doing it for four hundred rows would put an archive read per candidate on the keystroke that opened the popup.
</p>

<h2>Type names and their imports</h2>
<p>
  Typing a <strong>capitalised name</strong> (not after a dot) offers every class matching it across the JDK, your dependencies and your
  project, each with its package — and <em>(+N more)</em> when several packages declare the name. Within each match tier your project's types
  come before a jar's, and shorter names before longer.
</p>
<ul>
  <li>Accepting a name that maps to a <strong>single</strong> class <strong>adds its import</strong> — turn that off with Settings → Completion →
    <em>Auto-import on accept</em>. An ambiguous name inserts just the name; <kbd>Alt</kbd> + <kbd>Enter</kbd> → <strong>Import '…'</strong>
    picks the package.</li>
  <li><strong>A qualified name completes one segment at a time</strong>: <code>import org.</code> offers <code>springframework</code>, and
    <code>import org.springframework.boot.</code> offers its packages beside its classes.</li>
  <li>A receiver <strong>not imported yet</strong> still completes: <code>Arrays.</code> without the import offers its members, and accepting
    one adds the import too — for an unambiguous name only.</li>
  <li>A type receiver offers its <strong>nested types</strong> beside the statics — <code>Map.</code> offers <code>Entry</code>.</li>
</ul>
<p><strong>A nested type completed by its simple name is written through its outer</strong>, since <code>Inner</code> alone is not a name Java resolves:</p>
<pre><code>{@html highlightCode(`import it.acme.web.ConfigurazioneCors;
…
ConfigurazioneCors.MyProva prova;    // accepting "MyProva" writes this`, 'java')}</code></pre>
<p>
  One import serves every nested type of that class, the form people write by hand. Where the simple name is already in scope nothing is added:
  inside the outer class or a sibling nested in it, and in a file that imports the nested type outright.
</p>

<h3>Other languages</h3>
<p>
  In <strong>TypeScript</strong> and <strong>JavaScript</strong> — and in a <code>.svelte</code> file and an Angular project's templates — the popup
  is the language server's: <code>typescript-language-server</code>, <code>svelteserver</code>, <code>ngserver</code>. Install it from Settings →
  Language Servers; without one the file is still coloured, folded and edited, with nothing to suggest.
</p>
<Callout variant="info" title="A JSP's &lt;script&gt; is answered by Bennu">
  No language server will serve a JSP — a template that <em>prints</em> JavaScript, half of it <code>&lt;%= %&gt;</code> holes. The names the block
  declares (<code>var</code>, <code>function</code>, <code>foo: function (…)</code> and their parameters) come first, then the browser globals.
  After <code>document.</code>, <code>location.</code>, <code>Math.</code> and the rest of the platform, the members are <strong>read off the object
  itself</strong> in the engine the page runs in — not a list that could be stale. Hover says the same. What it cannot know is jQuery:
  <code>$</code> is offered, <code>$(…).</code> has no object to read.
</Callout>
