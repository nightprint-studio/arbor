<script lang="ts">
  /**
   * Message bundles: .properties bundles as a model — what counts as a key, go-to and hover per translation, missing keys,
   * and the Messages panel.
   */
  import Callout from '$lib/components/shared/ui/Callout.svelte';
</script>

<span class="eyebrow">Java &amp; JSP</span>
<h1>Message bundles</h1>

<p class="doc-lead">
  <code>.properties</code> bundles are resolved rather than read as text: a key used in code or in a page points at the line that defines it, in every language it is defined in.
</p>

<h2>Why they need a model</h2>
<p>
  Half of what a web application puts on screen is not in its source. It is in a <code>.properties</code> file, reached by a string — a string normally checked by
  nothing: not the compiler, not the tests, and, because Struts renders an unresolved key as the key itself, often not by anyone looking at the page either.
</p>

<h2>What counts as a key</h2>
<p>By shape rather than by a list of tags, because every framework in a legacy page spells it differently:</p>
<table>
  <thead><tr><th>Where</th><th>Example</th></tr></thead>
  <tbody>
    <tr><td>An attribute called <code>key</code></td><td><code>&lt;bean:message key="label.user"/&gt;</code></td></tr>
    <tr><td>An attribute whose name ends in <code>Key</code></td><td><code>titleKey</code>, <code>messageKey</code></td></tr>
    <tr><td>The <code>name</code> of an <code>&lt;s:text&gt;</code> — the one tag where <code>name</code> is a key</td><td><code>&lt;s:text name="label.user"/&gt;</code></td></tr>
    <tr><td>The first string argument, in Java</td><td><code>getText("…")</code>, <code>getMessage("…")</code>, <code>getString("…")</code></td></tr>
  </tbody>
</table>
<Callout variant="info" title="A computed value is not a key">
  <code>%&#123;keyName&#125;</code>, <code>$&#123;row.label&#125;</code> or a scriptlet usually is one at run time, but nothing can say which, and guessing would flag every
  dynamic label in the project.
</Callout>
<Callout variant="info" title="Entando's labels are left alone">
  <code>&lt;wp:i18n key="…"&gt;</code> reads the platform's label table in the <strong>database</strong>, edited from its admin console — no <code>.properties</code>
  declares it, and treating it as one would underline every label on every page. That one tag, by name, and no attempt at the rest of Entando's vocabulary.
</Callout>

<h2>On a key</h2>
<dl class="meta-grid">
  <dt><kbd>Ctrl</kbd> + <kbd>B</kbd></dt>
  <dd>Opens the line declaring it — one entry per translation, each showing what that language says, so choosing is reading.</dd>
  <dt>Hover</dt>
  <dd>The same without leaving the page, naming the locales that do not have the key yet.</dd>
  <dt>Completion</dt>
  <dd>Inside a key attribute, from the bundles, with each key's text beside it.</dd>
  <dt>A missing key</dt>
  <dd>A key <strong>no bundle declares</strong> is underlined where it is written.</dd>
</dl>

<h2>The Messages panel</h2>
<p>
  From the command palette. Every key with its default text and its bundle, and two things you cannot see any other way:
</p>
<div class="feature-grid two-col">
  <div class="feature-card">
    <div class="fc-eyebrow">How many places read it</div>
    <div class="fc-title">unused</div>
    <div class="fc-desc">When the answer is none.</div>
  </div>
  <div class="feature-card">
    <div class="fc-eyebrow">Which locales lack it</div>
    <div class="fc-title">missing</div>
    <div class="fc-desc">Counted per bundle — two bundles with different locale sets is normal, and comparing across them would invent a debt nobody has.</div>
  </div>
</div>
<p>
  Expanding a key shows every translation, each row opening its own file at its own line. Group by bundle or by key prefix.
</p>
