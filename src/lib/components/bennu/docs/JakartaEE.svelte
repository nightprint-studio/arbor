<script lang="ts">
  /**
   * Jakarta EE: CDI and EJB beans and the injection points they satisfy, the CDI beans panel, servlets in the
   * Endpoints panel, the deployment failures the source is enough to be sure of, and what is left alone.
   */
  import Callout from '$lib/components/shared/ui/Callout.svelte';
  import { highlightCode } from '$lib/utils/highlight';
</script>

<span class="eyebrow">Java &amp; JSP</span>
<h1>Jakarta EE</h1>

<p class="doc-lead">
  <code>@Inject OrderService orders;</code> says what it wants and nothing about what it gets. The container decides at deployment — and when the answer is
  none, or two, the application does not start. Bennu reads what the container reads and puts the answer beside the field.
</p>
<p>
  It switches on for a project that uses CDI, EJB or annotated servlets, in either the <code>javax</code> or the <code>jakarta</code> namespace. When the
  same project also wires beans with Spring, Spring answers <code>@Inject</code> and this page's injection features step aside; the EJB, proxy and servlet
  checks still run.
</p>

<h2>Beans and injection</h2>
<table>
  <thead><tr><th>Icon</th><th>Beside</th><th>Click to</th></tr></thead>
  <tbody>
    <tr><td><code>◆</code></td><td>A bean class or a <code>@Produces</code> method or field</td><td>See the injection points it satisfies</td></tr>
    <tr><td><code>→</code></td><td>An <code>@Inject</code> field, constructor or initializer parameter, or an <code>@EJB</code> field</td><td>See the beans that may satisfy it</td></tr>
  </tbody>
</table>
<dl class="meta-grid">
  <dt>Go to</dt>
  <dd><kbd>Ctrl</kbd> + <kbd>B</kbd> on an injection point's name opens its bean, or asks when several may answer.</dd>
  <dt>Hover</dt>
  <dd>On an injection point, the bean it gets or the candidates; on <code>@ApplicationScoped</code>, <code>@Stateless</code> and the rest, what that scope or bean kind means in one line.</dd>
  <dt>Completion</dt>
  <dd>Inside <code>@Named("…")</code> on an injection point, the names of the project's beans.</dd>
</dl>
<p>
  A bean is matched by type through the project's own hierarchy and by qualifier: <code>@Named</code>, the project's annotations marked
  <code>@Qualifier</code>, <code>@Default</code> and <code>@Any</code>. <code>@Singleton</code> from <code>javax.inject</code> and from
  <code>javax.ejb</code> are told apart, and a JAX-RS <code>@Produces</code> is never mistaken for a producer.
</p>
<Callout variant="tip" title="The gutter shows what may answer; the checks need certainty">
  An arrow lists every bean that could satisfy a point, including one whose qualifiers Bennu cannot fully read — you choose. A warning appears only when
  the source leaves no doubt.
</Callout>

<h2>The CDI beans panel</h2>
<p>
  Open <strong>Jakarta EE beans</strong> from the Command Palette. Every bean class and producer, badged by scope or EJB kind
  (<code>@ApplicationScoped</code>, <code>@Stateless</code>, <code>@Produces</code>, …), tagged with its <code>@Named</code> name, qualifiers and
  <code>alternative</code>, and grouped by scope or package. Expand a row to see the injection points it satisfies; each one opens its field.
</p>

<h2>Servlets in the Endpoints panel</h2>
<p>
  Every <code>@WebServlet</code> and <code>@WebFilter</code> URL pattern, and every <code>&lt;servlet-mapping&gt;</code> and
  <code>&lt;filter-mapping&gt;</code> in <code>web.xml</code>, is a row of the <strong>Endpoints</strong> panel — badged <code>SERVLET</code> or
  <code>FILTER</code>, beside the routes of every other framework the project uses.
</p>

<h2>The checks</h2>
<div class="feature-grid">
  <div class="feature-card">
    <div class="fc-eyebrow">Warning</div>
    <div class="fc-title">Unsatisfied injection</div>
    <div class="fc-desc">A point of one of the project's own types that no bean can satisfy — typically an implementation missing its scope annotation in an archive without <code>bean-discovery-mode="all"</code>.</div>
  </div>
  <div class="feature-card">
    <div class="fc-eyebrow">Warning</div>
    <div class="fc-title">Ambiguous injection</div>
    <div class="fc-desc">Two or more enabled beans certainly match. The message names them.</div>
  </div>
  <div class="feature-card">
    <div class="fc-eyebrow">Error</div>
    <div class="fc-title"><code>@Inject</code> on a final or static field</div>
    <div class="fc-desc">The container refuses the bean.</div>
  </div>
  <div class="feature-card">
    <div class="fc-eyebrow">Error</div>
    <div class="fc-title">Unproxyable bean</div>
    <div class="fc-desc">A normal-scoped bean reached through a proxy that cannot be built: a final class, a non-private final method, or no non-private constructor without parameters (relaxed for Quarkus).</div>
  </div>
  <div class="feature-card">
    <div class="fc-eyebrow">Error</div>
    <div class="fc-title">Invalid enterprise bean</div>
    <div class="fc-desc"><code>@Stateless</code>, <code>@Stateful</code>, <code>@Singleton</code> or <code>@MessageDriven</code> on an interface, or an abstract or final class.</div>
  </div>
  <div class="feature-card">
    <div class="fc-eyebrow">Error</div>
    <div class="fc-title">Servlets</div>
    <div class="fc-desc"><code>@WebServlet</code> on a class that is not a servlet, a URL pattern no container accepts, and one pattern claimed by two servlets in the same module — in the Java file and in <code>web.xml</code> alike.</div>
  </div>
</div>
<pre><code>{@html highlightCode(`public class OrderServiceImpl implements OrderService { }   // no scope: not a bean here

@RequestScoped
public class OrderController {
    @Inject OrderService orders;        // no bean in this project is assignable to OrderService
}

@ApplicationScoped
public final class Cache { }            // final: the client proxy cannot subclass it

@WebServlet({"orders", "/api/*"})       // "orders" must start with / or *.`, 'java')}</code></pre>

<h2>What it will not judge</h2>
<ul>
  <li><strong>Portable extensions</strong> add and veto beans at will: a project that implements one gets no injection warnings at all.</li>
  <li><strong>Library bean archives</strong> and producers of types Bennu cannot read may hold the missing bean, so a point of a library type is never
    reported unsatisfied.</li>
  <li><strong>Alternatives enabled at deployment</strong> are not in the source: any <code>@Alternative</code>, <code>@Priority</code>,
    <code>@Specializes</code> or <code>@Typed</code> bean of a type silences both injection warnings for it.</li>
  <li>Generic injection types, <code>@EJB</code> points, points of classes the container does not manage, and any qualifier or scope Bennu cannot
    classify (<code>@ViewScoped</code>, a library's own) are left alone.</li>
  <li>A <code>beans.xml</code> with <code>bean-discovery-mode="none"</code>, or modules that disagree, make which classes are beans unknowable — nothing
    about injection is claimed.</li>
  <li><strong>JSF</strong> managed beans and EL names in pages are not modelled.</li>
</ul>
