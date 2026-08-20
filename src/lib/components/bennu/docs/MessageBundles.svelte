<!-- Bennu docs — message bundles: properties files as a resolved resource. -->
<h1>Message bundles</h1>
<p class="doc-lead">
  <code>.properties</code> bundles are resolved rather than read as text: a key used in code or in
  a page points at the line that defines it, in every language it is defined in.
</p>

<h2>Message bundles</h2>
<p>
  Half of what a web application puts on screen is not in its source. It is in a
  <code>.properties</code> file, reached by a string, and normally that string is checked by
  nothing — not the compiler, not the tests, and, because Struts renders an unresolved key as the
  key itself, often not by anyone looking at the page either. Bennu treats bundles as a model.
</p>
<p>
  <strong>What counts as a key.</strong> By shape rather than by a list of tags, because every
  framework in a legacy page spells it differently: an attribute called <code>key</code>, an
  attribute whose name ends in <code>Key</code> (<code>titleKey</code>, <code>messageKey</code>),
  the <code>name</code> of a <code>&lt;s:text&gt;</code> — the one tag where <code>name</code> is
  a key rather than a field — and the first string argument of <code>getText</code>,
  <code>getMessage</code> or <code>getString</code> in Java. A <strong>computed</strong> value
  (<code>%&#123;keyName&#125;</code>, <code>$&#123;row.label&#125;</code>, a scriptlet) is not
  treated as a key at all: it usually is one at runtime, but nothing can say which, and guessing
  would flag every dynamic label in the project.
</p>
<p>
  <strong>What is deliberately left alone</strong>: a key that is answered from somewhere other
  than a file. Entando's <code>&lt;wp:i18n key="…"&gt;</code> reads the platform's label table in
  the <strong>database</strong>, edited from its admin console — no <code>.properties</code>
  declares it, and treating it as one would put a warning under every label on every page. That
  one tag, by name, and no attempt at the rest of Entando's vocabulary.
</p>
<p>
  <strong>On a key</strong>, <kbd>Ctrl</kbd> + <kbd>B</kbd> opens the line that declares it — one
  entry per translation, each showing what that language says, so choosing is reading rather than
  guessing. Hovering shows the same thing without leaving the page, and names the locales that do
  not have it yet. Typing inside a key attribute completes from the bundles, with each key's text
  beside it. A key <strong>no bundle declares</strong> is underlined where it is written.
</p>
<p>
  <strong>The Messages panel</strong> (command palette) lists every key with its default text, the
  bundle it belongs to, and two things you cannot see any other way: how many places read it —
  <code>unused</code> when the answer is none — and which locales are <code>missing</code> it.
  Expanding a key shows every translation; each row opens its own file at its own line. Group by
  bundle or by key prefix. Untranslated is counted per bundle, not project-wide: two bundles
  having different locale sets is normal, and comparing across them would invent a debt nobody
  has.
</p>
