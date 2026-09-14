<script lang="ts">
  /**
   * JAX-RS resources: routes in the shared Endpoints panel, the application path, the checks, and @PathParam completion / go-to.
   */
  import Callout from '$lib/components/shared/ui/Callout.svelte';
  import { highlightCode } from '$lib/utils/highlight';
</script>

<span class="eyebrow">Java &amp; JSP</span>
<h1>JAX-RS resources</h1>

<p class="doc-lead">
  Every route a project's <code>@Path</code> resources answer — the application path, the class path and the method path joined into the URL a request actually hits —
  and the mistakes the runtime only reports when that request arrives, or by refusing to deploy.
</p>

<h2>Routes in the Endpoints panel</h2>
<p>
  On the right activity bar, <kbd>Alt</kbd> + <kbd>4</kbd>: JAX-RS routes are listed in the same panel as Spring's request mappings and Struts actions, in the same shape
  — the full path, <code>Class#method</code>, the verb as a badge, the return type and <code>@Produces</code> as tags, and one expandable row per parameter with where
  its value comes from (<code>path</code>, <code>query</code>, <code>form</code>, <code>header</code>, <code>cookie</code>, <code>matrix</code>, <code>bean</code>,
  <code>context</code>, or <code>body</code> for the request entity). A parameter with <code>@DefaultValue</code> is tagged <em>optional</em>.
</p>
<pre><code>{@html highlightCode(`@ApplicationPath("api")
public class App extends Application {}

@Path("orders")
@Produces("application/json")
public class OrderResource {
    @GET @Path("{id}")                       // GET /api/orders/{id}
    public Order find(@PathParam("id") long id) { … }

    @Path("{id}/items")                      // LOCATOR /api/orders/{id}/items
    public ItemsResource items() { … }
}`, 'java')}</code></pre>
<ul>
  <li>Each resource method carries an <strong>endpoint</strong> mark in the gutter with its route as the tooltip; hovering its <code>@GET</code> or its
    <code>@Path</code> shows the route, the handler, and the media types it produces and consumes.</li>
  <li>A <strong>sub-resource locator</strong> — a <code>@Path</code> method with no verb — is its own <code>LOCATOR</code> row. What it returns is not joined onto it:
    the runtime decides that per request.</li>
  <li>Your own request-method annotations count when they are meta-annotated <code>@HttpMethod("X")</code>.</li>
  <li><code>jakarta.ws.rs</code> and <code>javax.ws.rs</code> are both recognised, through the file's imports — so CDI's <code>@Produces</code> and a project's own
    <code>@Path</code> are never mistaken for JAX-RS.</li>
</ul>

<h3>Annotated interfaces</h3>
<p>
  When the annotations live on an interface and the code on the one class that implements it, the route is listed on the <em>class</em> — that is what the runtime
  instantiates. The class's method takes the interface's annotations only while it carries none of its own: a single <code>@Produces</code> on the implementation
  discards them all, exactly as the runtime does. An interface nothing in the project implements is listed where it is written.
</p>

<h2>How the application path is found</h2>
<table>
  <thead><tr><th>Source</th><th>Read as</th></tr></thead>
  <tbody>
    <tr><td><code>@ApplicationPath("api")</code> on the <code>Application</code> subclass</td><td>The prefix <code>/api</code>, when it is the only value in the project</td></tr>
    <tr><td><code>web.xml</code> servlet mapping to <code>/rest/*</code></td><td>The prefix <code>/rest</code>, when nothing is annotated — for the Jersey or RESTEasy servlet, or a servlet named after an <code>Application</code> subclass</td></tr>
    <tr><td>Several distinct values, or a constant</td><td>No prefix, and every row tagged <em>application path unknown</em> rather than guessed</td></tr>
  </tbody>
</table>

<h2>The checks</h2>
<div class="feature-grid">
  <div class="feature-card">
    <div class="fc-eyebrow">Null on every request · warning</div>
    <div class="fc-title">A path parameter the route does not have</div>
    <div class="fc-desc"><code>@PathParam("id")</code> on a route with no <code>&#123;id&#125;</code> — nothing fails, the parameter is simply null. Reported on the string itself.</div>
  </div>
  <div class="feature-card">
    <div class="fc-eyebrow">Deployment refused · error</div>
    <div class="fc-title">Two methods for one route</div>
    <div class="fc-desc">Same verb, same class and method paths (variable names do not matter), same produces and consumes. Both methods are told, each naming the other.</div>
  </div>
  <div class="feature-card">
    <div class="fc-eyebrow">One body · error</div>
    <div class="fc-title">Several entity parameters</div>
    <div class="fc-desc">A request has one body, and it goes to the unannotated parameter. <code>@Context</code> and <code>@Suspended</code> are not entities.</div>
  </div>
  <div class="feature-card">
    <div class="fc-eyebrow">Silently skipped · warning</div>
    <div class="fc-title">A resource method that is not public</div>
    <div class="fc-desc">The runtime ignores it and logs nothing. Interface methods are public by definition and never reported.</div>
  </div>
  <div class="feature-card">
    <div class="fc-eyebrow">Usually empty · weak</div>
    <div class="fc-title">A body on GET or HEAD</div>
    <div class="fc-desc">Clients, proxies and caches routinely drop it.</div>
  </div>
</div>
<pre><code>{@html highlightCode(`@Path("orders/{orderId}")
public class OrderItems {
    @GET @Path("items/{itemId}")
    public Item get(@PathParam("orderId") long order,
                    @PathParam("id") long item) { … }   // no {id} in the route
}`, 'java')}</code></pre>

<h2>Completion and go-to on @PathParam</h2>
<p>
  Inside <code>@PathParam("…")</code> the completion list offers the variables of that method's route — application path, class path and method path — replacing the
  whole string. <kbd>Ctrl</kbd> + click on the string jumps to the <code>&#123;id&#125;</code> it binds, in the method's <code>@Path</code> or the class's.
</p>

<Callout variant="info" title="Two checks wait for the first scan">
  Whether a parameter names a variable depends on the class path, possibly inherited from an interface, and on every sub-resource locator in the project; whether a route is
  claimed twice depends on another file. Neither is reported before the project has been indexed. The other three are about one method and are reported immediately.
</Callout>

<h2>What it will not judge</h2>
<ul>
  <li>A <code>@Path</code> that is not one plain string literal — a constant, a concatenation. The route is still listed, with <code>…</code> for the part that cannot be read.</li>
  <li>A path parameter on a class with no class-level <code>@Path</code>, or one whose name any sub-resource locator's route declares: it may be bound through that
    locator.</li>
  <li>An interface implemented by more than one class, or re-annotated by its implementation — which annotations are in effect is not in the source.</li>
  <li>A parameter carrying an annotation Bennu does not know — Jersey's <code>@FormDataParam</code>, RESTEasy's <code>@MultipartForm</code> — is never counted as the
    entity.</li>
  <li>Clashes involving a route declared on an interface (usually a REST client mirroring the server), across build modules, between several applications, when the
    application lists its classes by hand, or when the regexes of two variables are written differently.</li>
</ul>
