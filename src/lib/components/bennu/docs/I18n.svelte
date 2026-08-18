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
