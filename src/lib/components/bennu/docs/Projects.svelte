<!-- Bennu docs — Projects, JDK & capabilities. -->
<h1>Projects, JDK &amp; capabilities</h1>
<p class="doc-lead">
  A Bennu project is a Maven project: the folder holding the root <code>pom.xml</code>. Opening it
  resolves the build model — the display name, the modules, the JDK language level — and scans for
  the domain frameworks the code relies on.
</p>

<h2>The JDK</h2>
<p>
  The footer shows the resolved Java language level and where it came from — usually
  <code>maven.compiler.source</code>, but also the compiler plugin, toolchains, or a manual
  override. When it can't be inferred the footer reads <code>JDK —</code>.
</p>

<h2>Capabilities</h2>
<p>
  Bennu detects the domain frameworks a project uses and shows the count in the footer. Each
  capability is backed by <strong>evidence</strong> at one of three tiers:
</p>
<ul>
  <li><strong>Tier A</strong> — a dependency coordinate in the <code>pom.xml</code> (strongest).</li>
  <li><strong>Tier B</strong> — a configuration file (e.g. a <code>struts.xml</code> or a TLD).</li>
  <li><strong>Tier C</strong> — a source pattern (corroborating; a C-only hit is provisional).</li>
</ul>
<p>
  The detected set gates which features light up — for example, JSP taglib awareness only when a
  taglib is actually in use. The demo project shows Struts (convention + XML), JSP taglibs, the
  OGNL value stack, a JDBC DAO and Entando.
</p>

<h2>Form analysis</h2>
<p>
  The <strong>Forms</strong> tool window (bottom dock, toggled from the right rail with
  <kbd>Alt</kbd> + <kbd>3</kbd>) analyses the open JSP and lists every <code>&lt;form&gt;</code>
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

<h2>Encodings</h2>
<p>
  Legacy projects often declare <code>Cp1252</code> in their <code>pom.xml</code>
  (<code>project.build.sourceEncoding</code>). Bennu decodes each file with the pom-declared
  encoding and shows which one won in the footer, so a mojibake surprise never slips in silently.
</p>
