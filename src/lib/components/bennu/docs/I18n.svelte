<!-- Bennu docs — the fulcrum engine's i18n convention. -->
<h1>i18n labels</h1>
<p class="doc-lead">
  A fulcrum project keeps its user-visible text out of the code: content declares a
  <strong>label</strong>, and the strings live in per-language TOML beside it. Bennu reads both
  sides, so a label that nothing declares, one that a language has forgotten, and one that nothing
  reads any more are all visible.
</p>

<h2>The layout</h2>
<p>Bennu recognises the convention by its shape — an <code>i18n/</code> directory with a
  <code>languages.toml</code> in it:</p>
<pre><code>{`i18n/
  languages.toml     the declared languages; the first enabled one is the fallback
  styles.toml        one table per style — what $red.bold{…} may name
  glossary.toml      one table per entry — what @potion{…} may name
  it/
    menu.toml        the file name IS the category
    tree.toml
  en/
    …`}</code></pre>
<p>
  A label is <code>category:dotted.key</code>: <code>menu:items.new_game</code> is
  <code>new_game</code> under <code>[items]</code> in <code>menu.toml</code>. The same label in
  <code>it/</code> and in <code>en/</code> is <strong>one label with two declarations</strong>.
</p>
<p>
  More than one tree is normal — the base project's and each mod's. They merge, later trees
  winning, exactly as the engine merges them. A language declared in one tree can be translated in
  another.
</p>
<p>
  Detection is by layout and not by dependency, deliberately: the tooling is useful on a project
  that only <em>authors</em> content — a <code>.ron</code> tree with its bundles beside it and the
  engine nowhere in its manifest — and the layout is what the engine itself keys on. Which signal
  convinced Bennu is listed under <strong>Projects &amp; capabilities</strong>.
</p>

<h2>The markup</h2>
<table>
  <thead><tr><th>Written</th><th>Means</th></tr></thead>
  <tbody>
    <tr><td><code>{'{amount}'}</code></td><td>a placeholder, interpolated when it is rendered</td></tr>
    <tr><td><code>{'$red.bold{…}'}</code></td><td>a style span; chainable, and <code>$mod:red&#123;…&#125;</code> for a namespaced one</td></tr>
    <tr><td><code>{'@potion{…}'}</code></td><td>a glossary reference; <code>@rpg:hp&#123;…&#125;</code> namespaced, <code>@status.protect&#123;…&#125;</code> dotted</td></tr>
    <tr><td><code>{'~sleep(0.8)'}</code>, <code>{'~slow{…}'}</code></td><td>a control — pacing or effect. Arguments and body are both optional</td></tr>
  </tbody>
</table>
<p>
  <code>\</code> escapes any of <code>$ @ ~ &#123; &#125; \</code>. A literal <code>$</code> has to be
  written <code>\$</code> — and in a <strong>literal</strong> TOML string (single quotes), because
  <code>"\$"</code> is not a valid TOML escape. Single quotes are the better default here for a
  second reason: Bennu can point at a problem <em>inside</em> a literal string, and inside a
  double-quoted one carrying escapes it can only point at the whole value.
</p>

<h2>What Bennu tells you</h2>
<p>In a <code>.ron</code> or a <code>.rs</code>, on a string that is a label:</p>
<ul>
  <li><strong>Hover</strong> — what it says, in every language it says it in, who has not translated
    it, and which placeholders it expects.</li>
  <li><strong>Go to declaration</strong> — one target per language, landing on the text.</li>
  <li><strong>Completion</strong> — the labels that continue what you are typing, with the text
    each one resolves to.</li>
  <li><strong>A warning when no bundle declares it.</strong> This is the check that pays for the
    rest: a mistyped label is invisible to the compiler and to every test, and the engine renders
    the label itself when it cannot resolve one — so it survives QA until somebody notices
    <code>tree:nodes.drill.name</code> written on screen.</li>
</ul>
<p>In a bundle:</p>
<ul>
  <li><strong>Markup problems</strong> — an unclosed <code>{'$bold{'}</code>, a <code>$</code> with no
    style name, a control with unbalanced parentheses.</li>
  <li><strong>A style or glossary entry that does not exist</strong>, on the name itself. Checked
    only when the project <em>has</em> a <code>styles.toml</code> — one that does not has not
    written a wrong name.</li>
  <li><strong>Missing in the fallback language</strong> — a label declared in another language but
    not in the default one, which makes the fallback itself fail.</li>
</ul>
<p>
  Missing in the <em>other</em> languages is a tag in the Labels panel rather than a warning: on a
  project mid-translation that would be a warning per label, burying the mistyped ones under the
  merely unfinished ones.
</p>

<h2>The i18n panel</h2>
<p class="doc-lead">
  A bundle is a file where TOML sees one thing — a string — and the markup inside it is invisible to
  every tool that reads TOML. The panel is the other half of that file:
  <kbd>Alt</kbd> + <kbd>Shift</kbd> + <kbd>I</kbd>, or the <em>i18n</em> button on the editor toolbar,
  which appears on a translation file and nowhere else. It opens on the right, beside the editor: the
  markup on the left, what it comes out as on the right, both changing as you type.
</p>
<p>
  It follows the <strong>caret</strong>. There is no label to pick — the line you are on is the
  translation, and a second selection to keep in step with the first would be one too many.
</p>

<h3>The preview</h3>
<p>
  The sentence, with the constructs resolved: styles painted from <code>styles.toml</code>, glossary
  terms marked as terms, and each placeholder shown as its name until you give it a sample value.
</p>
<p>
  It shows the <strong>distinctions</strong> the stylesheet draws — that <code>$warning</code> is not
  the colour of <code>$hint</code>, that a title is bigger than the text around it — and not the
  engine's own output. Sizes are relative: the smallest declared size renders at the panel's own, the
  rest in proportion. A faithful 48-point heading in a side panel would push the rest off screen, and
  what you are checking is how the sentence reads, which survives being scaled.
</p>
<p>
  Two things it will not pretend to: a <strong>control</strong> is drawn as a chip rather than as an
  animation, because <code>~shake</code> is motion and a still picture of it would be a lie; and a
  <strong>style or glossary name the project does not declare</strong> is underlined rather than
  recoloured, because the real consequence is that the span has lost its styling.
</p>

<h3>Parameters</h3>
<p>
  Every parameter of the label — <em>not</em> only the ones this language uses. That union is the
  point: <code>en</code> passing <code>{'{amount}'}</code> while the Italian never mentions it is a
  real defect, invisible to every compiler and every test because both files are valid, and invisible
  in any view that shows one language at a time. The row says which languages use it, and the button
  writes it in at the caret.
</p>
<p>
  Typing a <strong>sample value</strong> substitutes it into the preview, which is what turns "the
  markup, minus the markup" into "the sentence, as somebody will read it" — and long values are the
  reason to bother: <code>{'{name}'}</code> reads fine until it is <em>Bartolomeo della Fortezza</em>
  and the line wraps into three. Samples are scratch; they are not saved.
</p>

<h3>Writing markup</h3>
<p>
  Four buttons in the panel's header, each wrapping the editor's selection — or opening an empty
  construct with the caret inside it when there is no selection. With a selection the words stay
  selected afterwards, so <code>$red.bold&#123;…&#125;</code> is two presses on the same words.
</p>
<ul>
  <li><strong>Style</strong> and <strong>glossary</strong> offer only what the project declares. A
    name that is not in <code>styles.toml</code> is a defect, so it is not offered.</li>
  <li><strong>Control</strong> and <strong>placeholder</strong> accept anything you type — a control
    name is whatever the engine implements, and i18n knows the <em>form</em> of
    <code>~slow&#123;…&#125;</code> and nothing about its meaning. The controls your project already
    uses are listed first, most-used first, since that is the only honest list there is. Typing
    <code>sleep(0.8)</code> or <code>sleep 0.8</code> both write
    <code>~sleep(0.8)</code>.</li>
</ul>
<p>
  On a <strong>double-quoted value containing an escape</strong> the buttons are disabled and the
  panel says why: such a string's content is shorter than its source, so no offset inside it can be
  trusted, and Bennu will not write to a byte it cannot locate. Rewriting the value with single quotes
  fixes the toolbar, the colouring and the problem markers at once.
</p>

<h3>Switching language</h3>
<p>
  The picker beside the label lists <strong>every declared language</strong>, and the ones with no
  translation yet are the ones it exists for: picking one opens the file the translation would go in,
  even when that file does not exist yet. Languages that are declared but switched off stay in the
  list, marked — a translation may legitimately go there, but nothing is owed to it.
</p>
<p>
  Below the parameters, the languages that <em>do</em> have the label show what they say, so
  "what does the English actually say" is answered without opening anything.
</p>

<h2>Markup colouring in the editor</h2>
<p>
  In the bundle itself, the parts of a value that are structure rather than prose are coloured: the
  placeholder names, the style names, the glossary keys, the controls, with a tint over each construct
  so nesting reads as nesting. A name the project does not declare is coloured as a warning
  <em>as you type</em> — the diagnostic saying the same thing arrives with the next scan, and the
  failure it prevents is silent, since a style that does not exist renders as the default and nothing
  complains.
</p>

<h2>The Labels panel</h2>
<p>
  Open it from the command palette (<em>i18n labels</em>). One row per label — the label, the text
  it resolves to, the category as its badge — and tags saying how many places read it and which
  languages are missing it.
</p>
<p>Expanding a row gives two kinds of child, and between them they answer three questions:</p>
<ul>
  <li>one per <strong>language</strong>: what it says there, clicking jumps to the declaration;</li>
  <li>one per <strong>reading</strong>: which file and which line, clicking jumps to the use.</li>
</ul>
<p>
  So <em>find usages</em>, <em>find unused</em> and <em>which languages are missing</em> are the same
  list with a filter, which is how they are actually used — you look at the set, not at one label at
  a time. Filter for <code>unused</code> to find the labels content deleted and left behind; filter
  for <code>missing</code> to find what a translator still owes. Group by category or by key prefix.
</p>
<p>
  A label read from hundreds of content files shows the first fifty readings; the count on the row
  is the real one.
</p>
