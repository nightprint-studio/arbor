<script lang="ts">
  /**
   * i18n labels: the fulcrum convention — the layout, labels and their markup, what Bennu checks on either side, the i18n
   * panel beside a bundle, and the Labels panel across the project.
   */
  import Callout from '$lib/components/shared/ui/Callout.svelte';
</script>

<span class="eyebrow">Java &amp; JSP</span>
<h1>i18n labels</h1>

<p class="doc-lead">
  A fulcrum project keeps its user-visible text out of the code: content declares a <strong>label</strong>, and the strings live in per-language TOML beside it. Bennu reads
  both sides, so a label nothing declares, one a language has forgotten, and one nothing reads any more are all visible.
</p>

<h2>The layout</h2>
<p>Bennu recognises the convention by its shape — an <code>i18n/</code> directory with a <code>languages.toml</code> in it:</p>
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
  A label is <code>category:dotted.key</code>: <code>menu:items.new_game</code> is <code>new_game</code> under <code>[items]</code> in <code>menu.toml</code>. The same label in
  <code>it/</code> and in <code>en/</code> is <strong>one label with two declarations</strong>.
</p>
<p>
  More than one tree is normal — the base project's and each mod's. They merge, later trees winning, exactly as the engine merges them, and a language declared in one tree can be
  translated in another.
</p>
<Callout variant="info" title="Detected by layout, not by dependency">
  The tooling is useful on a project that only <em>authors</em> content — a <code>.ron</code> tree with its bundles and the engine nowhere in its manifest — and the layout is what
  the engine itself keys on. Which signal convinced Bennu is listed under <strong>Projects</strong>.
</Callout>

<h2>The markup</h2>
<table>
  <thead><tr><th>Written</th><th>Means</th></tr></thead>
  <tbody>
    <tr><td><code>{'{amount}'}</code></td><td>A placeholder, interpolated when rendered</td></tr>
    <tr><td><code>{'$red.bold{…}'}</code></td><td>A style span — chainable, and <code>$mod:red&#123;…&#125;</code> for a namespaced one</td></tr>
    <tr><td><code>{'@potion{…}'}</code></td><td>A glossary reference — <code>@rpg:hp&#123;…&#125;</code> namespaced, <code>@status.protect&#123;…&#125;</code> dotted</td></tr>
    <tr><td><code>{'~sleep(0.8)'}</code>, <code>{'~slow{…}'}</code></td><td>A control, pacing or effect; arguments and body both optional</td></tr>
  </tbody>
</table>
<p>
  <code>\</code> escapes any of <code>$ @ ~ &#123; &#125; \</code>.
</p>
<Callout variant="tip" title="Prefer single-quoted strings">
  A literal <code>$</code> is written <code>\$</code> — in a <strong>literal</strong> TOML string, since <code>"\$"</code> is not a valid TOML escape. And Bennu can point at a problem
  <em>inside</em> a literal string, while inside a double-quoted one carrying escapes it can only point at the whole value.
</Callout>

<h2>What Bennu tells you</h2>
<div class="feature-grid two-col">
  <div class="feature-card">
    <div class="fc-eyebrow">In a <code>.ron</code> or a <code>.rs</code></div>
    <div class="fc-title">On a string that is a label</div>
    <div class="fc-desc">
      <strong>Hover</strong>: what it says in every language, who has not translated it, which placeholders it expects. <strong>Go to declaration</strong>: one target per language.
      <strong>Completion</strong>: the labels continuing what you type, with their text. And <strong>a warning when no bundle declares it</strong>.
    </div>
  </div>
  <div class="feature-card">
    <div class="fc-eyebrow">In a bundle</div>
    <div class="fc-title">On the markup</div>
    <div class="fc-desc">
      <strong>Markup problems</strong> — an unclosed <code>{'$bold{'}</code>, a <code>$</code> with no style name, unbalanced parentheses. <strong>A style or glossary entry that does not
      exist</strong>, on the name. <strong>Missing in the fallback language</strong>, which makes the fallback itself fail.
    </div>
  </div>
</div>
<Callout variant="warning" title="The check that pays for the rest">
  A mistyped label is invisible to the compiler and to every test, and the engine renders the label itself when it cannot resolve one — so it survives QA until somebody notices
  <code>tree:nodes.drill.name</code> written on screen.
</Callout>
<ul>
  <li>A missing style or glossary name is checked only when the project <em>has</em> a <code>styles.toml</code> — one that does not has not written a wrong name.</li>
  <li>Missing in the <em>other</em> languages is a tag in the Labels panel, not a warning: mid-translation that would be a warning per label, burying the mistyped ones under the unfinished.</li>
</ul>

<h2>The i18n panel</h2>
<p>
  A bundle is a file where TOML sees a string, and the markup inside it is invisible to every tool that reads TOML. The panel is the other half of that file:
  <kbd>Alt</kbd> + <kbd>Shift</kbd> + <kbd>I</kbd>, or the <em>i18n</em> button on the toolbar of a translation file. It opens beside the editor — the markup on the left, what it
  comes out as on the right, both changing as you type.
</p>
<p>
  It follows the <strong>caret</strong>. There is no label to pick: the line you are on is the translation.
</p>

<h3>The preview</h3>
<p>
  The sentence with its constructs resolved: styles painted from <code>styles.toml</code>, glossary terms marked, each placeholder shown as its name until you give it a sample.
</p>
<p>
  It shows the <strong>distinctions</strong> the stylesheet draws — that <code>$warning</code> is not the colour of <code>$hint</code>, that a title is bigger than the text around it —
  not the engine's output. Sizes are relative: the smallest declared renders at the panel's own size, the rest in proportion, since a faithful 48-point heading in a side panel would
  push everything off screen.
</p>
<Callout variant="info" title="What it will not pretend">
  A <strong>control</strong> is a chip, not an animation — <code>~shake</code> is motion, and a still of it would lie. A <strong>style or glossary name the project does not
  declare</strong> is underlined rather than recoloured, because the real consequence is that the span has lost its styling.
</Callout>

<h3>Parameters</h3>
<p>
  Every parameter of the label — <em>not</em> only the ones this language uses. That union is the point: <code>en</code> passing <code>{'{amount}'}</code> while the Italian never
  mentions it is a real defect, invisible to compilers and tests because both files are valid, and invisible in any view showing one language at a time. The row says which languages
  use it, and its button writes it in at the caret.
</p>
<p>
  A <strong>sample value</strong> is substituted into the preview — "the sentence, as somebody will read it" — and long ones are the reason to bother: <code>{'{name}'}</code> reads fine
  until it is <em>Bartolomeo della Fortezza</em> and the line wraps into three. Samples are scratch, never saved.
</p>

<h3>Writing markup</h3>
<p>
  Four buttons in the header wrap the editor's selection — or open an empty construct with the caret inside when nothing is selected. The words stay selected afterwards, so
  <code>$red.bold&#123;…&#125;</code> is two presses on the same words.
</p>
<dl class="meta-grid">
  <dt>Style, glossary</dt>
  <dd>Offer only what the project declares: a name not in <code>styles.toml</code> is a defect.</dd>
  <dt>Control, placeholder</dt>
  <dd>Accept anything — a control is whatever the engine implements, and i18n knows its <em>form</em>, not its meaning. The controls the project uses come first, most-used first. <code>sleep(0.8)</code> and <code>sleep 0.8</code> both write <code>~sleep(0.8)</code>.</dd>
</dl>
<Callout variant="warning" title="Disabled on a double-quoted value with escapes">
  Such a string's content is shorter than its source, so no offset inside it can be trusted, and Bennu will not write to a byte it cannot locate. The panel says why — and rewriting the
  value with single quotes fixes the toolbar, the colouring and the problem markers at once.
</Callout>

<h3>Switching language</h3>
<p>
  The picker beside the label lists <strong>every declared language</strong>, and the ones with no translation yet are what it is for: picking one opens the file the translation would go
  in, even when it does not exist yet. Languages declared but switched off stay listed, marked. Below the parameters, the languages that <em>do</em> have the label show what they say.
</p>

<h2>Markup colouring in the editor</h2>
<p>
  In the bundle, the parts of a value that are structure rather than prose are coloured — placeholder names, style names, glossary keys, controls — with a tint over each construct so
  nesting reads as nesting. A name the project does not declare is coloured as a warning <em>as you type</em>; the diagnostic follows with the next scan, and the failure it prevents is
  silent, since a style that does not exist renders as the default.
</p>

<h2>The Labels panel</h2>
<p>
  From the command palette, <em>i18n labels</em>. One row per label — the label, its text, its category as a badge — with tags saying how many places read it and which languages lack it.
  Expanding a row gives two kinds of child:
</p>
<ul>
  <li>one per <strong>language</strong> — what it says there; clicking jumps to the declaration;</li>
  <li>one per <strong>reading</strong> — which file and line; clicking jumps to the use.</li>
</ul>
<p>
  So <em>find usages</em>, <em>find unused</em> and <em>which languages are missing</em> are one list with a filter, the way they are really used. Filter for <code>unused</code> to find
  labels content deleted and left behind, for <code>missing</code> to find what a translator still owes. Group by category or key prefix. A label read from hundreds of files shows the
  first fifty readings; the count on the row is the real one.
</p>
