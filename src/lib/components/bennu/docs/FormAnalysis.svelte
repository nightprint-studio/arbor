<script lang="ts">
  /**
   * Form analysis: the Forms panel — what each form on a JSP submits, where it goes, and which of its fields bind.
   */
  import Callout from '$lib/components/shared/ui/Callout.svelte';
</script>

<span class="eyebrow">Java &amp; JSP</span>
<h1>Form analysis</h1>

<p class="doc-lead">
  For a page with forms on it, the <strong>Forms</strong> panel reads what each one submits and where — the fields, what they post, and the action on the other end.
</p>

<h2>The Forms panel</h2>
<p>
  In the bottom dock, toggled from the right rail with <kbd>Alt</kbd> + <kbd>3</kbd> — offered only on a project that has JSP pages. It analyses the open JSP and
  lists every <code>&lt;form&gt;</code> relevant to it, each with the <strong>complete set of parameters</strong> it posts.
</p>
<dl class="meta-grid">
  <dt>Where it goes</dt>
  <dd>The action the form targets — the mapped action class, and the <code>struts.xml</code> fragment declaring it — even when written as an Entando <code>&lt;wp:action path=…&gt;</code>. The config button opens that fragment.</dd>
  <dt>What it posts</dt>
  <dd>Every input, <strong>hidden</strong> ones included, with the <code>value</code> each posts — a fixed value or a <code>$&lbrace;…&rbrace;</code> / <code>%&lbrace;…&rbrace;</code> expression.</dd>
  <dt>When it posts it</dt>
  <dd>A field inside <code>&lt;c:if&gt;</code> or <code>&lt;s:if&gt;</code> is marked <strong>if</strong> — hover for the condition — since it is submitted only when that holds.</dd>
</dl>
<p>Clicking a form or a field jumps the editor to it. Two badges mark each field:</p>
<div class="feature-grid two-col">
  <div class="feature-card">
    <div class="fc-title">bound</div>
    <div class="fc-desc">The field name is a writable property of the action class.</div>
  </div>
  <div class="feature-card">
    <div class="fc-title">valid</div>
    <div class="fc-desc">The field carries a Struts validation rule.</div>
  </div>
</div>
<Callout variant="warning" title="A field with neither badge">
  It reads as muted — the signal that the name is a typo, or a request parameter nothing maps.
</Callout>

<h2>Forms split across includes</h2>
<p>
  A JSP form is often split across <code>&lt;jsp:include&gt;</code>s: the page opens the <code>&lt;form&gt;</code>, and the hidden tokens, the wizard-step inputs
  and the button bar come from fragments. The panel follows both directions:
</p>
<ul>
  <li><strong>On a parent page</strong>, each form gathers the fields its includes contribute, each tagged with the fragment it comes from.</li>
  <li><strong>On an included fragment</strong>, the parent form it feeds surfaces — a chip names the page it lives on — with its whole parameter set, and the fields
    <em>this</em> fragment contributes highlighted.</li>
</ul>
<p>
  The walk is recursive and cycle-safe, and a very large include graph shows a "…more" hint rather than silently dropping pages.
</p>
