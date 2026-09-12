<script lang="ts">
  /**
   * The editor: what a file looks like and does once it is open — languages, tabs, Markdown, folding, the surface,
   * formatting, imports, hints, pasting into strings, saving, and files changed underneath you. Moving around is
   * Navigation; what is offered while typing is Completion.
   */
  import Callout from '$lib/components/shared/ui/Callout.svelte';
  import { highlightCode } from '$lib/utils/highlight';
</script>

<span class="eyebrow">Editor</span>
<h1>The editor</h1>

<p class="doc-lead">
  What a file looks like and does once it is open — highlighting, folding, guides, the overview ruler, and what
  happens when you save it or when something else changes it underneath you. Moving <em>around</em> the code is
  <strong>Navigation</strong>; what the editor offers as you type is <strong>Completion</strong>.
</p>

<h2>Languages</h2>
<p>How much the editor knows about a file depends on the language, in four tiers:</p>
<table>
  <thead><tr><th>Tier</th><th>Languages</th><th>What you get</th></tr></thead>
  <tbody>
    <tr>
      <td><strong>A real grammar</strong></td>
      <td>Java, JSP, <code>.dig</code></td>
      <td>Semantic highlighting and folding. Java and JSP add navigation and index-backed completion; <code>.dig</code>
        completes from its own vocabulary.</td>
    </tr>
    <tr>
      <td><strong>Highlight and fold</strong></td>
      <td>HTML, JSON</td>
      <td>Colour and folding.</td>
    </tr>
    <tr>
      <td><strong>Ready for a server</strong></td>
      <td>Rust, C, C++, Python, Lua, Go</td>
      <td>A colouring mode of their own, so a <code>.py</code> build script, a plugin's <code>main.lua</code> or a
        <code>.cpp</code> beside a JNI library reads as code with no server at all — and gains completion, hover and
        semantic colour the moment clangd, pyright, lua-language-server or gopls is installed.</td>
    </tr>
    <tr>
      <td><strong>Colour only</strong></td>
      <td>TOML, RON, XML, YAML, <code>.properties</code>, CSS/SCSS/LESS, JavaScript/TypeScript, shell, Windows batch
        (<code>.bat</code>, <code>.cmd</code>), Dockerfile, SQL</td>
      <td>Highlighting. Navigation and completion want a language server, and until one is wired those actions are
        hidden rather than offered and silent.</td>
    </tr>
  </tbody>
</table>
<p>Markdown is a case of its own — below. A few languages have details worth knowing:</p>
<dl class="meta-grid">
  <dt>Dockerfile</dt>
  <dd>
    Matched by <em>name</em>, so <code>Dockerfile</code>, the <code>Dockerfile.dev</code> a project gains once it builds more
    than one image, <code>api.dockerfile</code> and Podman's <code>Containerfile</code> all read the same.
  </dd>
  <dt>DTD</dt>
  <dd>
    <code>.dtd</code>, and the <code>.ent</code> and <code>.mod</code> fragments a large one is split into, have a mode of
    their own. A DTD is not XML — <code>&lt;!ELEMENT</code> is a malformed tag to an XML highlighter — and it is what the
    <code>struts.xml</code>s and <code>.tld</code>s of a legacy project are written against. The declarations, the name
    each one <em>declares</em> and the parameter entities (<code>%common;</code>) a real DTD is mostly made of each read apart.
  </dd>
  <dt>SQL</dt>
  <dd>
    Highlighted per <strong>dialect</strong>, because the engines disagree about quoting: Oracle's <code>q'[…]'</code> and
    PostgreSQL's <code>$$ … $$</code> are each a broken string under the other's rules, and getting it wrong paints the rest
    of the file as one literal. Nothing in a <code>.sql</code> file says which engine it targets, so it is a setting —
    <strong>Settings → Editor → SQL → Dialect</strong>. The default, <em>Portable</em>, uses the rules valid on both.
  </dd>
</dl>

<h2>Tabs</h2>
<ul>
  <li>A file opened from the <strong>Project</strong> tree or a <strong>Find in project</strong> hit joins the tab strip.</li>
  <li>A click switches tabs; the <strong>×</strong> closes one, and a neighbour takes the focus.</li>
  <li>A tab keeps its cursor, its scroll position and its <strong>undo history</strong> while it is open, so coming back to
    a file lands you where you left it and <kbd>Ctrl/Cmd</kbd> + <kbd>Z</kbd> still takes back what you typed there. Closing
    the tab lets them go.</li>
</ul>

<h2>Markdown</h2>
<p>
  A <code>.md</code> opens <strong>rendered</strong>, in the same live-preview editor Garrulus's notes use: headings are
  sized, links read as their titles, tables are tables, images and video appear, and fenced code is highlighted by language —
  including the ones this app is built around and no Markdown renderer has heard of: <code>dig</code>, <code>merula</code>,
  <code>wgsl</code>, <code>ron</code>.
</p>
<p>
  It is still the file and still an editor: put the caret on a line and that line shows its markup, so a typo is fixed where
  you found it. The button in the toolbar — or <em>Markdown: edit the source</em> in the command palette — switches to the
  code editor for when the markup itself is the work, and the choice is remembered.
</p>
<p>
  The <strong>source</strong> view colours its fences from the same vocabulary: a <code>bash</code>, <code>sh</code>,
  <code>zsh</code> or <code>console</code> block reads as shell in either view, a <code>bat</code>, <code>cmd</code> or
  <code>batch</code> one as Windows batch, and so do <code>java</code>, <code>xml</code>, <code>yaml</code>,
  <code>dockerfile</code> and the rest of what a README quotes.
</p>

<h3>Alerts</h3>
<p>An alert renders as a callout — a coloured band with an icon and a title:</p>
<pre><code>{@html highlightCode(`> [!WARNING]
> Deploying this on a Friday is how the weekend ends.`, 'markdown')}</code></pre>
<p>
  GitHub's five (<code>NOTE</code>, <code>TIP</code>, <code>IMPORTANT</code>, <code>WARNING</code>, <code>CAUTION</code>)
  and the Obsidian words that mean the same things (<code>info</code>, <code>hint</code>, <code>success</code>,
  <code>attention</code>, <code>danger</code>, <code>question</code>, <code>example</code>, <code>quote</code>) are
  recognised, in any case. Anything written after the marker becomes the callout's title.
</p>

<h3>Tables</h3>
<p>
  <strong>A table stays a table.</strong> Click a cell and you edit that cell — only the cell you are in shows its markdown,
  everything around it stays rendered. The strip under the table adds and removes rows and columns and sets a column's
  alignment; <kbd>Tab</kbd> and <kbd>Enter</kbd> walk the grid and <kbd>Esc</kbd> leaves it. The toolbar's
  <strong>⊞</strong> inserts a new one — point at the grid to say how big.
</p>
<Callout variant="info" title="An edited table is normalised">
  Editing a table rewrites its markdown with one space each side of every cell, so a hand-aligned table loses its padding the
  first time it is edited here.
</Callout>

<h3>Links</h3>
<dl class="meta-grid">
  <dt>Following one</dt>
  <dd>
    <kbd>Ctrl</kbd> / <kbd>Cmd</kbd> + click — the gesture that follows a symbol in code. An <code>http</code> link opens in
    the browser; a path opens the file in a tab, resolved against the document's own folder, so <code>./notes/api.md</code>
    means the one beside it; a <code>#anchor</code> jumps to that heading, in this file or the one it opens. Bare URLs count,
    both <code>&lt;https://…&gt;</code> and one written on its own. A plain click puts the caret where you clicked, and the hand
    cursor appears only while the key is held, so it never promises a jump a click alone won't make.
  </dd>
  <dt>Writing one</dt>
  <dd>
    Typing <code>](</code> offers every file in the project, written relative to this document, and every heading in it —
    from the list the jump reads, so an id that is offered lands. Typing <code>#</code> first narrows it to the headings,
    each showing its title beside its id.
  </dd>
</dl>
<p>
  <strong>A heading needs no id: its text is one.</strong> Lower-cased, punctuation dropped, spaces turned into dashes —
  GitHub's rule, character for character, so an anchor copied from a table of contents generated there lands:
</p>
<table>
  <thead><tr><th>Heading</th><th>Answers to</th></tr></thead>
  <tbody>
    <tr><td><code>## Perché il CST</code></td><td><code>#perché-il-cst</code>, and <code>#perche-il-cst</code> — accents may be left out</td></tr>
    <tr><td><code>## Titolo</code>, twice</td><td><code>#titolo</code> and <code>#titolo-1</code></td></tr>
    <tr><td><code>## Pipelines — CI / CD</code></td><td><code>#pipelines--ci--cd</code>, and <code>#pipelines-ci-cd</code> without the doubled dashes</td></tr>
  </tbody>
</table>
<p>
  An underlined heading has an id like any other, a <code>path.md#anchor</code> works the same way in the file it opens, and a
  <code>#</code> comment inside a fenced code block is never mistaken for a heading.
</p>

<h3>Diagrams</h3>
<p>
  A <code>mermaid</code> fence is <strong>drawn</strong> — flowcharts, sequence diagrams, state machines — in the theme's own
  colours. The caret inside it brings the source back, <strong>highlighted</strong>: the arrows, the node labels and the
  diagram type each read apart. A <code>.mmd</code> file opens with the same colouring, and a diagram that does not parse says
  so where the picture would have been, in mermaid's words, naming the line.
</p>

<h2>Folding</h2>
<p>
  Braced blocks — classes, methods, blocks — and block comments fold from the gutter chevrons, keeping the head line visible.
  In an indentation-delimited language like <code>.dig</code> the body folds from the end of its header line instead. Folding is
  computed live from the syntax tree, with no indexing needed.
</p>
<Callout variant="tip" title="Collapse the comments as a file opens">
  <strong>Settings → Editor → Folding</strong> can fold a file's block comments on opening — the licence header and the
  documentation above every method, which on a legacy file can be most of the screen. It is an opening state, not a rule:
  nothing folds again what you unfold. The same section turns the gutter off entirely.
</Callout>

<h2>What the surface shows</h2>
<div class="feature-grid">
  <div class="feature-card">
    <div class="fc-eyebrow">Settings → Editor → Appearance</div>
    <div class="fc-title">Appearance</div>
    <div class="fc-desc">The font size, the line-number gutter, the tint on the caret's line, word wrap, and spaces and tabs drawn as glyphs — each applied to the file already in front of you.</div>
  </div>
  <div class="feature-card">
    <div class="fc-eyebrow">Always on</div>
    <div class="fc-title">Rainbow brackets</div>
    <div class="fc-desc">Every <code>()</code>, <code>[]</code> and <code>&#123;&#125;</code> is tinted by nesting depth, a matching pair sharing a colour. Brackets inside strings and comments are left alone.</div>
  </div>
  <div class="feature-card">
    <div class="fc-eyebrow">Settings → Editor → Indentation guides</div>
    <div class="fc-title">Indentation guides</div>
    <div class="fc-desc">A line per indent level, in the colour of the bracket that opens the block; the block the caret is in is fully highlighted.</div>
  </div>
  <div class="feature-card">
    <div class="fc-eyebrow">Settings → Editor → Sticky scroll</div>
    <div class="fc-title">Sticky scroll</div>
    <div class="fc-desc">Scrolling into a long body pins the enclosing declarations — the class, then the method — to the top. Click a pinned line to jump back to it.</div>
  </div>
  <div class="feature-card">
    <div class="fc-eyebrow">Settings → Editor → Scrollbar overview</div>
    <div class="fc-title">Scrollbar overview</div>
    <div class="fc-desc">Every error and warning is a bar at its place in the file. Hover to preview that spot, drag to scroll, click a mark to put the caret there ready for a quick fix.</div>
  </div>
  <div class="feature-card">
    <div class="fc-eyebrow">The top-right corner</div>
    <div class="fc-title">File health</div>
    <div class="fc-desc">The file's error and warning counts — a green check when it is clean — mirroring the marks on the overview strip.</div>
  </div>
</div>
<p>
  The overview marks include the <strong>compiler's</strong> errors and warnings, not only the live analysis: after a build,
  whatever <code>javac</code>, Maven or <code>cargo</code> reported for the open file is a mark on the strip and a squiggle on the
  line. A rebuild replaces them, so a fixed error clears its mark.
</p>

<h2>Indentation</h2>
<p>
  The footer shows the active indentation as <em>Spaces: N</em> or <em>Tab Size: N</em>. Click it — or focus it and press
  <kbd>↑</kbd>/<kbd>↓</kbd> — to switch between <strong>spaces and tabs</strong> and pick the <strong>tab width</strong>
  (2, 4 or 8). The change applies to the open editor at once.
</p>

<h2>Reformat</h2>
<p>
  <kbd>Alt</kbd> + <kbd>Shift</kbd> + <kbd>F</kbd> reformats the open file. A language with a <strong>language server</strong> is
  formatted by it — Rust by <code>rustfmt</code>, reading the project's own <code>rustfmt.toml</code> or <code>.prettierrc</code>,
  where the team and the CI read it from. <strong>Java</strong> is formatted by Bennu:
</p>
<ul>
  <li>every line re-indented to its nesting, with the indentation the footer shows;</li>
  <li>trailing whitespace stripped;</li>
  <li>runs of blank lines collapsed.</li>
</ul>
<p>
  Its style is <strong>Settings → Java → Code Style → Formatter</strong>: how many consecutive blank lines survive between members
  (0 removes them all), and whether the statements under a <code>case</code> label are indented from it. The indentation itself
  comes from Editor → Indentation, so the formatter and the editor cannot disagree, and both persist — a file formats the same
  way tomorrow.
</p>
<Callout variant="info" title="It deliberately stops there">
  It never rewraps a long line, reorders anything, adds or removes braces, or changes the spacing inside an expression. Those are
  the rules that can be wrong — <code>a &lt; b</code> and <code>Map&lt;K, V&gt;</code> differ by context, <code>-1</code> and
  <code>a - 1</code> by parse — and a formatter that occasionally rewrites an expression is one nobody dares run on inherited
  code. Comments and text blocks are left exactly as written, and a file that does not parse still formats.
</Callout>

<h2>Optimize imports</h2>
<p>
  <kbd>Ctrl</kbd> + <kbd>Shift</kbd> + <kbd>O</kbd> on a Java file drops the imports the file does not use and puts the rest in
  order: everything else first, then <code>javax</code> and <code>java</code>, then the static imports — alphabetical inside each
  group, a blank line between groups, duplicates collapsed, as one undo step.
</p>
<pre><code>{@html highlightCode(`import java.util.List;
import org.slf4j.Logger;
import static org.junit.Assert.assertEquals;
import com.acme.Order;
import java.util.List;`, 'java')}</code></pre>
<pre><code>{@html highlightCode(`import com.acme.Order;
import org.slf4j.Logger;

import java.util.List;

import static org.junit.Assert.assertEquals;`, 'java')}</code></pre>
<p>
  What counts as unused is the judgement the <code>unused-import</code> warning makes, so the command and the squiggle can never
  disagree — and it inherits that caution: an import named only in a Javadoc counts as used, and a <code>static</code> or wildcard
  import is never removed, only moved. It will not fold imports of one package into a wildcard, which can change what a simple name
  resolves to, and it will not add a missing import — that is <kbd>Alt</kbd> + <kbd>Enter</kbd>'s.
</p>
<Callout variant="info" title="A comment among the imports stops it">
  A comment sits above the import it was written for, and reordering would strand it above another — so such a file is left alone.
</Callout>

<h2>Parameter and inlay hints</h2>
<p>
  Inside a call's argument list, a strip above the line shows the <strong>signature</strong> with the argument you are on picked
  out, following the commas as you type. Bennu answers it for Java, the language server for everything else.
</p>
<p>
  It describes the <em>call</em>, not the declaration: the receiver's type arguments are filled in, so
  <code>Optional&lt;PathPattern&gt;.orElseThrow(…)</code> reads <code>: PathPattern</code> rather than <code>: T</code>. A type
  variable the receiver does not bind — one the call's own arguments decide — stays as written.
</p>
<p><strong>Inlay hints</strong> (Settings → Editor) draw what the code does not say:</p>
<ul class="prop-list">
  <li><strong>Parameter names</strong><code>transfer(source: from, target: to, amount: 500)</code> — in front of each argument that does not already carry the name</li>
  <li><strong>Inferred types</strong>what a <code>var</code> or a Lombok <code>val</code> was inferred as</li>
  <li><strong>Lambda parameters</strong><code>rows.forEach(row: String -&gt; …)</code> — for one written without a type</li>
</ul>
<p>
  They are not part of the file: the caret cannot land in one, they are not copied with a selection, and no offset shifts. An
  argument that already says the name, a lambda or a long expression is left alone. <strong>Rest the pointer on a parameter
  name</strong> and it shows the type the parameter is declared with.
</p>
<p>
  The type hint includes the <strong>primitives</strong>, which are the hardest to work out from the line:
  <code>var n = path.indexOf('/')</code> is an <code>int</code>, <code>var half = a / 2</code> an <code>int</code> and not a
  <code>double</code>, <code>var c = chars[i]</code> a <code>char</code>. A type the engine cannot work out gets no hint rather
  than a guess — a hint reads as though the compiler had said it.
</p>
<p>
  An <strong>overloaded</strong> method still gets its names when the arguments settle which overload it is:
  <code>addAllowedMethod("*")</code> against <code>(HttpMethod)</code> and <code>(String)</code> is decided by the literal. When
  they do not — an argument whose type does not resolve, a <code>null</code> that fits both — the call gets no names, because a
  name from the wrong overload is a claim about the code that is not true.
</p>
<Callout variant="tip" title="A library's parameter names come from its sources">
  They appear once the sources are on disk — the JDK's own, and any dependency whose sources you fetched with the decompiled
  view's <em>Download sources</em>. A class file carries no parameter names unless compiled with <code>-parameters</code>, which
  almost no published jar is, and a decompiled stub's <code>arg0</code> is the decompiler's placeholder. Sources fetched while
  you work take effect at once, in every file that calls that library.
</Callout>

<h2>Emmet</h2>
<p>
  In JSP and HTML files, type an <strong>Emmet abbreviation</strong> and press <kbd>Tab</kbd> to expand it into markup. When the
  caret is not on a valid abbreviation, <kbd>Tab</kbd> indents as usual.
</p>
<table>
  <thead><tr><th>Typed</th><th>Becomes</th></tr></thead>
  <tbody>
    <tr><td><code>ul&gt;li.item*3</code></td><td>a list of three items</td></tr>
    <tr><td><code>div#app</code></td><td>a div with an id</td></tr>
    <tr><td><code>a[href]</code></td><td>a link</td></tr>
  </tbody>
</table>

<h2>Pasting into a string</h2>
<p>
  Paste inside a Java <code>"…"</code> and the text is <strong>escaped</strong> as it lands: quotes and backslashes are escaped,
  tabs become <code>\t</code>. Something that <strong>spans several lines</strong> becomes concatenated literals, one per line,
  aligned under the opening quote and joined with <code>+</code> — the shape you would have typed, since a <code>"…"</code>
  cannot span lines in Java:
</p>
<pre><code>{@html highlightCode(`String q = "SELECT *\\n" +
           "FROM \\"user\\"";`, 'java')}</code></pre>
<ul>
  <li>In a <strong>text block</strong> (<code>"""</code>) newlines stay newlines, pasted lines are indented to match the block, and
    only the quotes that would close it early are escaped.</li>
  <li>In a <code>'…'</code> character literal the text is escaped and never split.</li>
  <li>Everywhere else — in code, in a comment, just past the closing quote — a paste arrives exactly as it left.</li>
</ul>
<dl class="meta-grid">
  <dt>Past 500 lines</dt>
  <dd>Still escaped, no longer split: the newlines stay inline in one literal. A concatenation of thousands of pieces nests thousands of levels deep, more than the tools that read it can walk.</dd>
  <dt>Past 64&nbsp;KB</dt>
  <dd>Refused, with a note at the caret. That is the most a compiled string constant can hold however it is written — the compiler joins the pieces back into one constant — so no arrangement of that text would build.</dd>
</dl>

<h2>Saving</h2>
<p>
  <strong>Autosave is on by default</strong>: a changed file is written a short moment after you stop typing, when you switch tab,
  and when the window loses the focus. <kbd>Ctrl</kbd> + <kbd>S</kbd> still saves on demand, in the project's encoding — and a
  rename applies and saves its edits the same way. Turn autosave off in <strong>Settings → Editor → Autosave</strong> to save only
  on <kbd>Ctrl</kbd> + <kbd>S</kbd>; the choice persists.
</p>

<h2>Files changed outside Bennu</h2>
<p>
  Bennu watches the files you have open — another editor, a <code>git checkout</code>, a code generator, a build — and never
  writes over a change it did not make.
</p>
<div class="feature-grid two-col">
  <div class="feature-card">
    <div class="fc-eyebrow">No unsaved edits in the tab</div>
    <div class="fc-title">Picked up silently</div>
    <div class="fc-desc">There is nothing to lose and nothing to decide.</div>
  </div>
  <div class="feature-card">
    <div class="fc-eyebrow">Unsaved edits in the tab</div>
    <div class="fc-title">Bennu asks</div>
    <div class="fc-desc"><strong>Keep my edits</strong> overwrites the disk; <strong>Reload from disk</strong> discards yours; <em>Not now</em> defers the choice.</div>
  </div>
</div>
<p>
  While a file waits on that decision its tab is badged <strong>disk</strong> and <strong>autosave is paused for it</strong>, so an
  unattended timer cannot pick a side for you. Every other tab keeps autosaving.
</p>
<Callout variant="info" title="The save itself is guarded too">
  A write whose file moved underneath is <strong>refused</strong> rather than applied, and the toast says so — for
  <kbd>Ctrl</kbd> + <kbd>S</kbd>, autosave, save on tab switch, and the writes a <strong>Rename</strong> makes across files.
</Callout>
<p>
  A file <strong>deleted</strong> under an edited buffer is not a conflict: your buffer is the last copy, and saving recreates the file.
</p>
