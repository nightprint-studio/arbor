<script lang="ts">
  /**
   * Navigation: find, back and forward, go to line, go to declaration, find usages, usage counts and what nothing
   * reaches, call and type hierarchy, and hover. Everything answers from the index, not from the open tabs.
   */
  import Callout from '$lib/components/shared/ui/Callout.svelte';
  import { highlightCode } from '$lib/utils/highlight';
</script>

<span class="eyebrow">Editor</span>
<h1>Navigation</h1>

<p class="doc-lead">
  Getting to the thing you are thinking of, from wherever you are. Everything here works from the keyboard, and everything
  answers from the index rather than from the open tabs — so it finds what you have never opened.
</p>

<h2>Find</h2>
<p>
  <kbd>Ctrl</kbd> + <kbd>F</kbd> searches the current file. <kbd>Ctrl</kbd> + <kbd>Shift</kbd> + <kbd>F</kbd> opens
  <strong>Find in project</strong>: a search across the whole project, with <strong>Match case</strong>, <strong>Whole
  word</strong> and <strong>Regex</strong> beside the field — and on a workspace a fourth toggle reaching every project of it.
  Hits are grouped by file with the match highlighted, and stream in as the scan finds them, so a large project fills the list
  rather than making you wait.
</p>
<p>
  The selected hit is shown <strong>in context</strong> beside the list — the lines around it, match highlighted — which is what
  tells four identical-looking lines apart without opening four files. <kbd>↑</kbd>/<kbd>↓</kbd> move and the preview follows;
  <kbd>Enter</kbd> opens. A word selected in the editor pre-fills the field, here and in find-in-file.
</p>
<p>The header row decides <strong>what is searched</strong>:</p>
<dl class="meta-grid">
  <dt>Source</dt>
  <dd><strong>Project</strong>, <strong>Project &amp; dependencies</strong> or <strong>Dependencies</strong> — whose text is read.</dd>
  <dt>Module</dt>
  <dd>On a multi-module build, one module.</dd>
  <dt>File mask</dt>
  <dd><code>*.java</code>, or several at once as <code>*.jsp, *.tag</code>.</dd>
</dl>
<p>
  Module and mask filter what came back rather than what is scanned, so changing either re-lists instantly without searching again,
  and both are <strong>remembered per project</strong>. The count between them says how many matches survived.
</p>
<Callout variant="tip" title="Searching inside the dependency jars">
  Reading the jars' XML, schemas, tag libraries and property files is how you find which artifact declares the interceptor or the
  bean you are looking at. Those hits are <strong>tinted</strong>, named by their <strong>artifact</strong>, arrive after the
  project's own, and open read-only. It is chosen per search rather than as a setting, because every candidate entry has to be
  decompressed to be read — a cost you take for the question you are asking now.
</Callout>

<h2>Back, forward, and where you have been</h2>
<table>
  <thead><tr><th>Shortcut</th><th>Goes</th></tr></thead>
  <tbody>
    <tr><td><kbd>Ctrl</kbd> + <kbd>Alt</kbd> + <kbd>←</kbd></td><td><strong>Back</strong> to where you jumped from</td></tr>
    <tr><td><kbd>Ctrl</kbd> + <kbd>Alt</kbd> + <kbd>→</kbd></td><td><strong>Forward</strong> again</td></tr>
    <tr><td><kbd>Ctrl</kbd> + <kbd>Shift</kbd> + <kbd>Backspace</kbd></td><td>Back to where you were <strong>typing</strong>; again for the edit before</td></tr>
    <tr><td><kbd>Ctrl</kbd> + <kbd>Shift</kbd> + <kbd>E</kbd></td><td><strong>Recent locations</strong>, as a list</td></tr>
  </tbody>
</table>
<Callout variant="info" title="What counts as a place you went">
  A stop is recorded when an <strong>action</strong> navigates — a go-to, a usage, a structure or find hit, a diagnostic, a switch
  to another tab — and never when the caret merely moves. Arrow keys, a click, page-down and scrolling are reading, so the history
  stays a list of places you chose. Each jump remembers <strong>both ends</strong>, so the first Back lands exactly where you left,
  down to the column, and navigating after a Back starts a new branch the way a browser does.
</Callout>
<p>
  Typing has its own history, because "where was I reading" and "where was I editing" are different questions.
</p>
<p>
  <strong>Recent locations</strong> shows the history most recent first, each row with the line you were on. Type to filter by file
  name or by the text of the line, <kbd>↑</kbd>/<kbd>↓</kbd> to move, <kbd>Enter</kbd> to go. <strong>Edited only</strong> keeps the
  places you changed — how you find the file you were working on before the interruption, instead of pressing Back five times.
</p>

<h2>Go to line</h2>
<p>
  <kbd>Ctrl</kbd> + <kbd>G</kbd> — type <code>42</code>, or <code>42:8</code> for line and column, and press <kbd>Enter</kbd>.
</p>

<h2>Go to declaration</h2>
<p>
  <kbd>Ctrl</kbd> + <kbd>B</kbd>, <kbd>Ctrl</kbd> + click, or the right-click menu. Where it lands depends on what the caret is on:
</p>
<table>
  <thead><tr><th>The caret on</th><th>Lands on</th></tr></thead>
  <tbody>
    <tr><td>A Java class, method, field or local</td><td>Its declaration</td></tr>
    <tr><td><code>helper</code> in <code>Reports::helper</code>, <code>run</code> in <code>this::run</code></td><td>The method — a method reference is a use of it</td></tr>
    <tr><td><code>Reports</code> in <code>Reports::helper</code></td><td>The type</td></tr>
    <tr><td>A declaration itself — a signature, a variable, class or record declaration</td><td>Its <strong>usages</strong>, since jumping there would go nowhere</td></tr>
    <tr><td>A JSP action reference — <code>action="…"</code>, <code>/do/Category/viewTree</code></td><td>Where the action is declared: the Struts config fragment, or its view JSP; for one that only resolves to a class, the class name is shown</td></tr>
    <tr><td>A Struts <code>&lt;result&gt;</code> JSP path, <code>/WEB-INF/x.jsp</code></td><td>That JSP</td></tr>
    <tr><td>A Struts OGNL or EL result, <code>$&#123;urlErrori&#125;</code></td><td>The action's property</td></tr>
    <tr><td>A library or JDK method — <code>list.add(…)</code>, <code>LOGGER.info(…)</code>, <code>String::valueOf</code></td><td>That library's source view, on the method itself</td></tr>
    <tr><td>A library or JDK type with no source</td><td>A <strong>decompiled stub</strong></td></tr>
  </tbody>
</table>
<p>
  It answers from the project index and the configuration graph, so it works once the index is warm and stays quiet when a symbol
  cannot be resolved. In a Struts config, a JSP path that does not exist under the web app, or an OGNL root that is not a property of
  the action, is flagged with a warning.
</p>
<p>
  A library method's receiver is typed against the project's classpath, so it works on anything your dependencies resolve to, and it
  chains: from inside one library view you can go on to the next.
</p>
<Callout variant="info" title="A decompiled stub">
  Generated from the bytecode: the type declaration and every field and method signature, with a header saying it is decompiled —
  method bodies are not stored in a class file. It is cached, read-only, and not validated, since without bodies validation would
  only report noise.
</Callout>

<h2>Find usages</h2>
<p>
  <kbd>Alt</kbd> + <kbd>F7</kbd> on a class, method or field lists every place it is used across the project, in a popover — pick one to
  jump there. It answers once the index is warm.
</p>
<p>A field counts <strong>every</strong> read of it, with or without a receiver:</p>
<pre><code>{@html highlightCode(`class Counter {
    private int count;

    void inc() {
        count++;                 // a use of the field
    }

    void setCount(int count) {   // the parameter, not the field
        this.count = count;      // a use of the field
    }
}`, 'java')}</code></pre>
<p>
  <code>this.count</code>, <code>other.count</code>, <code>Config.MAX</code> and the bare <code>count</code> meaning
  <code>this.count</code> are the same field — and that bare shape is the usual one, often the only one for a
  <code>static final</code> constant. A local or parameter of the same name is that variable, not the field it hides. A declaration
  is never a use of itself.
</p>
<Callout variant="tip" title="The usages of a Svelte component">
  In a <code>.svelte</code> file, <kbd>Alt</kbd> + <kbd>Shift</kbd> + <kbd>F7</kbd> lists the usages of <strong>the component
  itself</strong> — its imports and <code>&lt;Foo /&gt;</code> tags. The file is the component, so there is no name inside it to put
  a caret on.
</Callout>

<h2>Usage counts, and what nothing reaches</h2>
<p>
  Above a class, method or field, a line says how many places use it. It reads the same whole-project index as find usages, so it
  counts uses in files you have never opened, and pressing the line opens the list it counted. Turn it off in
  <strong>Settings → Editor → Usage counts</strong>.
</p>
<p>
  A declaration nothing reaches has its <strong>name drawn faint</strong> — a claim deliberately harder to earn than a count of zero,
  because a name faded wrongly invites deleting working code. Zero uses is a fact about the index; unused is a fact about the program.
  So a declaration is one of three things:
</p>
<div class="feature-grid">
  <div class="feature-card">
    <div class="fc-eyebrow">Count shown · faded at zero</div>
    <div class="fc-title">Nothing outside the code reaches it</div>
    <div class="fc-desc">The count is the whole story. This is the case the feature exists for.</div>
  </div>
  <div class="feature-card">
    <div class="fc-eyebrow">Count shown · never faded</div>
    <div class="fc-title">Something might</div>
    <div class="fc-desc">It carries an annotation the engine does not recognise — <code>@Deprecated</code>, one of your own — so no claim is safe. A deprecated method nobody calls is exactly what you were looking for.</div>
  </div>
  <div class="feature-card">
    <div class="fc-eyebrow">Nothing drawn at zero</div>
    <div class="fc-title">Something does, by design</div>
    <div class="fc-desc">A <code>@Test</code> run by JUnit, a <code>@Bean</code> built by Spring, a <code>@GetMapping</code> the dispatcher calls, a <code>@PrePersist</code>, <code>main</code>, an override. A row saying "no usages" above every test method is true and useless.</div>
  </div>
</div>
<p>
  The last kind still shows a count when there <em>is</em> one: a <code>@Bean</code> method called from another <code>@Bean</code>
  method is an ordinary call, and the number is worth having. Silence is only ever for a zero.
</p>
<p>
  <strong>A type whose members a framework calls is one itself</strong> — which is how a test class goes quiet without anybody guessing
  from its name. A class holding <code>@Test</code> methods is one JUnit instantiates; the same covers a configuration full of
  <code>@Bean</code> factories and a controller full of mappings. A <code>TestUtils</code> with no test in it is an ordinary class.
</p>
<p>
  A constructor gets no count at all: its callers are <code>new</code> expressions, which the index keys by the type.
</p>
<Callout variant="info" title="The same question Safe delete asks">
  What is faded — a plain declaration, no annotation, no supertype, nothing calling it — is exactly what <em>Safe delete</em> would
  agree to remove. One question, asked once, so the colour and the refactoring cannot disagree.
</Callout>

<h2>Call and type hierarchy</h2>
<table>
  <thead><tr><th>Shortcut</th><th>Opens</th></tr></thead>
  <tbody>
    <tr><td><kbd>Ctrl</kbd> + <kbd>Shift</kbd> + <kbd>H</kbd></td><td>The <strong>callers</strong> of the method at the caret</td></tr>
    <tr><td><kbd>Ctrl</kbd> + <kbd>H</kbd></td><td>The <strong>type hierarchy</strong> of the class you are in, the caret anywhere inside it</td></tr>
  </tbody>
</table>
<p>
  Both land in the Hierarchy panel at the bottom, whose direction chip walks the other way — callees, or what a type is built on. The
  tree expands a level at a time, so a recursive chain is walked as far as you care to.
</p>
<p>
  It is the index find usages reads, which is why the two agree — and why the callers of a method declared on an
  <strong>interface</strong> include the ones written against an implementation, and the other way round: the family a rename carries.
  A caller row jumps to the <em>call</em>, not to the head of the method containing it, and says <code>2×</code> when there is more than
  one. A call in a field initialiser or a static block is filed under its class, as a leaf.
</p>
<Callout variant="warning" title="Two limits">
  Calls into a <strong>dependency</strong> are not there: the index records an edge only when the callee is the project's own code.
  And <strong>overloads collapse</strong> — <code>process(String)</code> and <code>process(int)</code> are one row, because the index
  keys a method by name. A supertype from a jar does appear in a type hierarchy and can be expanded upward, but has no source to jump to.
</Callout>

<h2>Hover</h2>
<p>
  Rest the pointer on a class, method or field for a card saying what it is — a tag: class, interface, enum, method, field — its
  signature, and the type that <em>declares</em> it, which is the supertype for an inherited member. It answers from the index, once warm.
</p>
<dl class="meta-grid">
  <dt>The top</dt>
  <dd>
    The signature, the package, and for a type out of a jar the <strong>dependency</strong> as <code>groupId:artifactId:version</code>.
    The package says <em>which</em> type; the coordinate says <em>whose</em> — on a long classpath the question actually asked. These stay
    put while a long card <strong>scrolls</strong>; move the pointer onto the card to scroll it.
  </dd>
  <dt>The documentation</dt>
  <dd>From your own source, or from the library's <code>-sources.jar</code> or the JDK's <code>src.zip</code> when on disk. A dependency without downloaded sources shows the signature alone — a <code>.class</code> carries no comments — until <em>Download sources</em> on its decompiled view.</dd>
  <dt>The signature</dt>
  <dd>Written as Java, generics and <code>throws</code> included: <code>&lt;X extends Throwable&gt; T orElseThrow(Supplier&lt;? extends X&gt;) throws X</code>.</dd>
</dl>

<h3>A Javadoc is rendered, not printed</h3>
<p>
  A doc comment is HTML — that is what the language says it is — so its headings are headings, its <code>&lt;pre&gt;</code> examples
  are code blocks that scroll sideways rather than wrap, its lists are lists, and <code>&amp;#064;</code> is an <code>@</code>.
</p>
<ul>
  <li><code>&lbrace;@link …&rbrace;</code> reads as the member it points at, <code>&lbrace;@code …&rbrace;</code> as code.</li>
  <li><code>@param</code>, <code>@return</code> and <code>@throws</code> are lifted into a labelled list; <code>@deprecated</code> is
    highlighted, since it changes what you do next. An <code>@Override</code> inside an example stays in the example.</li>
  <li>Angle brackets that are not markup are left alone: <code>Vec&lt;T&gt;</code> and <code>Map&lt;K, V&gt;</code> are type parameters.</li>
  <li>Code blocks are <strong>coloured</strong> with the grammars a fenced block in a rendered <code>.md</code> gets: Java for a Javadoc,
    the file's language for what a language server documents.</li>
</ul>

<h3>Variables, and JSP</h3>
<p>
  Hovering a <strong>variable</strong> — a local, a parameter, a loop variable, a <code>catch</code> parameter, a pattern variable —
  names its type, its <strong>fully-qualified</strong> type (which of the four <code>Order</code>s this one is) and <strong>what that
  type is</strong>: class, interface, enum, record or annotation. A <code>var</code> or a Lombok <code>val</code> never shows as
  <code>var</code>: the card shows the type the compiler deduced, including the element type in <code>for (val row : rows)</code>.
</p>
<p>
  In a JSP, hovering a form field, an OGNL reference or a <code>*-validation.xml</code> <code>&lt;field&gt;</code> shows the
  <strong>type</strong> of the matching property on the bound action class, with the action it belongs to.
</p>
