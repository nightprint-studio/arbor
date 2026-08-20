<!-- Bennu docs — form analysis: what a JSP page submits, and where it goes. -->
<h1>Form analysis</h1>
<p class="doc-lead">
  For a page with forms on it, the <strong>Forms</strong> panel reads what each one submits and
  where — the fields, their types, and the action on the other end.
</p>

<h2>Form analysis</h2>
<p>
  The <strong>Forms</strong> tool window (bottom dock, toggled from the right rail with
  <kbd>Alt</kbd> + <kbd>3</kbd> — offered only on a project that actually has JSP pages) analyses
  the open JSP and lists every <code>&lt;form&gt;</code>
  relevant to it with the <strong>complete set of parameters</strong> it posts. It resolves the action each form targets — the mapped action class
  and the <code>struts.xml</code> fragment that declares it, even when the target is written as an
  Entando <code>&lt;wp:action path=…&gt;</code> — and lists every input, including
  <strong>hidden</strong> ones, with the <code>value</code> each posts (a fixed value or an
  <code>$&lbrace;…&rbrace;</code>/<code>%&lbrace;…&rbrace;</code> expression). A field inside a
  <code>&lt;c:if&gt;</code>/<code>&lt;s:if&gt;</code> is marked <strong>if</strong> (hover for the
  condition) since it is submitted only when that test holds. Two badges flag each field:
  <strong>bound</strong> when the field name is a writable property of the action class, and
  <strong>valid</strong> when it carries a Struts validation rule. A field that is neither reads as
  muted — the signal a name is a typo or an unmapped request parameter. Clicking a form or field
  jumps the editor to it; the config button opens the declaring fragment.
</p>
<p>
  It is <strong>include-aware</strong>. A JSP form is often split across
  <code>&lt;jsp:include&gt;</code>s — the page opens the <code>&lt;form&gt;</code> and the hidden
  tokens, wizard-step inputs and button bar come from included fragments. So each form gathers those
  fields too: on a parent page you see all the parameters, the children's included, each tagged with
  the fragment it comes from. And the reverse — when you are on an included fragment, the parent form
  it feeds surfaces (a chip names the page it lives on) with its whole parameter set, and the fields
  <em>this</em> fragment contributes are highlighted. The walk is recursive and cycle-safe; a very
  large include graph shows a "…more" hint rather than silently dropping pages.
</p>
