<script lang="ts">
  /**
   * Tag libraries: .tld files read from the project and from the dependency jars, what that gives a page, where it stays
   * quiet, and editing a .tld itself.
   */
  import Callout from '$lib/components/shared/ui/Callout.svelte';
</script>

<span class="eyebrow">Java &amp; JSP</span>
<h1>Tag libraries</h1>

<p class="doc-lead">
  The tags you write come from <code>.tld</code> files, and most of them live inside dependency jars. Bennu reads both, so a taglib tag stops being opaque text.
</p>

<h2>What a page gets</h2>
<p>
  Bennu reads the <code>.tld</code> files a page declares — the project's own, and the ones inside the <strong>dependency jars</strong>, which is where the tags you actually
  write come from.
</p>
<div class="feature-grid two-col">
  <div class="feature-card">
    <div class="fc-eyebrow">Typing</div>
    <div class="fc-title">Completion</div>
    <div class="fc-desc"><code>&lt;s:</code> lists that library's tags; inside a tag, its attributes minus those already written. A <code>uri="…"</code> in a directive completes from every library the project can resolve.</div>
  </div>
  <div class="feature-card">
    <div class="fc-eyebrow">Resting the pointer</div>
    <div class="fc-title">Hover</div>
    <div class="fc-desc">The TLD's own prose — the tag's description, an attribute's type, whether it is required, whether it accepts a runtime expression. On a legacy library, often the only documentation there is.</div>
  </div>
  <div class="feature-card">
    <div class="fc-eyebrow"><kbd>Ctrl</kbd> + <kbd>B</kbd></div>
    <div class="fc-title">Go to the TLD</div>
    <div class="fc-desc">On the <code>uri</code>, the <strong>TLD</strong> — one inside a jar included; on a tag name, its <code>&lt;tag&gt;</code> declaration; on an attribute, its <code>&lt;attribute&gt;</code>.</div>
  </div>
  <div class="feature-card">
    <div class="fc-eyebrow">Squiggles</div>
    <div class="fc-title">Checks</div>
    <div class="fc-desc">A tag the library does not declare, an attribute it does not have, a required attribute missing, a <code>uri</code> nothing on the classpath ships.</div>
  </div>
</div>

<h2>Where it stays silent</h2>
<Callout variant="info" title="Quiet where it cannot be sure">
  A project whose dependencies have not resolved reports nothing rather than everything. A prefix the page never declared is never flagged — it usually comes from an
  included fragment, invisible from the page. And a tag declaring <code>dynamic-attributes</code>, or written as a <code>.tag</code> file, has an attribute list that is
  <em>unknown</em> rather than empty.
</Callout>

<h2>Editing a .tld</h2>
<p>
  A <code>.tld</code> is itself edited with completion and checks, against a tag-library grammar that is <strong>built in</strong>. Both generations are covered — the JSP
  1.1 spellings (<code>tagclass</code>, <code>bodycontent</code>) beside the modern ones.
</p>
<Callout variant="info" title="Why built in">
  A TLD names its schema at <code>java.sun.com</code>, and the only copy sits inside a servlet container's jars — <code>provided</code> scope, often absent. The file that
  defines a project's whole tag vocabulary was the one XML file with no vocabulary of its own.
</Callout>
