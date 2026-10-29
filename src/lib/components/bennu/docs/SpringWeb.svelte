<script lang="ts">
  /**
   * Spring web: the Endpoints panel (Spring routes and Struts actions in one list), what a route expands into, and the
   * checks that compare endpoints with each other.
   */
  import Callout from '$lib/components/shared/ui/Callout.svelte';
  import { highlightCode } from '$lib/utils/highlight';
</script>

<span class="eyebrow">Spring</span>
<h1>Spring web</h1>

<p class="doc-lead">
  A web application is, among other things, a list of URLs — and in Spring that list exists nowhere as text. It is assembled at startup from annotations spread over
  every controller. Bennu assembles it while you edit, shows it as one list, and checks it against itself.
</p>

<h2>The Endpoints panel</h2>
<p>
  On the right activity bar, <kbd>Alt</kbd> + <kbd>4</kbd>: every URL the application answers, <em>whoever routes it</em>. A Spring route arrives with its class and
  method mappings joined; a <strong>Struts action</strong> as its URL — the package namespace joined to the action name — with the bean id resolved to the class that
  runs. An application mid-migration has both, in one list.
</p>
<ul>
  <li>Group by path, by handler or by method; filter across paths, handlers, return types <em>and</em> parameter names, with a count of what survived.</li>
  <li>Each route expands to what it takes: which values come from the path, the query string or the body, which are optional, and what each is called when renamed.</li>
  <li>Verbs are coloured as an API console colours them, and the <code>{'{'}variables{'}'}</code> in a path lit apart, so the list is skimmable.</li>
  <li>Media types are shown by <strong>short names</strong> — <code>JSON</code>, <code>SSE</code>, <code>XML</code>, <code>form</code> — rather than
    <code>MediaType.APPLICATION_JSON_VALUE</code>; an unfamiliar one keeps its spelling, being the one worth seeing.</li>
</ul>
<p>
  A request handler also carries a <code>»</code> in the gutter of its own file, which shows its route. The panel is offered only where the project has routes: a batch
  job gets no Endpoints button, no <kbd>Alt</kbd> + <kbd>4</kbd> and no palette entry, and the button appears as soon as a rebuild finds the first one.
</p>

<h3>An action expands into the whole request</h3>
<p>
  Under a Struts row are the action's own <code>&lt;interceptor-ref&gt;</code>s, then one row per <code>&lt;result&gt;</code>: its name, its type, the target the config
  names and — when different — the page it finally reaches:
</p>
<pre><code>tiles   admin.Cat.tree  →  /WEB-INF/jsp/tree.jsp</code></pre>
<p>
  The definition name alone says nothing about which file you will open, and following it by hand means the action fragment, then <code>tiles.xml</code>, then the
  parent definition. Clicking opens the page. A <code>chain</code> or a redirect names another action and says so. An action with no interceptor rows does not override
  its package's default stack — it does have interceptors.
</p>

<h3>A type is a door</h3>
<p>
  A chip naming a composite type — a route's return type, a parameter's — expands into that class's <strong>fields</strong>, and each composite field expands in turn,
  as deep as you click. An interface or a class from a jar is listed by the <strong>properties its getters expose</strong>, and says so. Wrappers are seen through:
  <code>ResponseEntity&lt;OrderDto&gt;</code> opens on <code>OrderDto</code>. Nothing is resolved until you ask.
</p>

<h3>Export</h3>
<p>
  <strong>Export</strong> — the ⭳ in the header — takes the list out as CSV, JSON or a Markdown table, to the clipboard or a file: what is on screen, filter and grouping
  applied, with each route's parameters flattened onto its row.
</p>

<h2>Endpoints checked against each other</h2>
<p>
  <em>Does this handler's <code>@PathVariable</code> match its path?</em> fits in a file. <em>Does another controller already claim this route?</em> does not — that one is
  in another package, maybe written three years ago.
</p>
<pre><code>{@html highlightCode(`@GetMapping("/orders/{orderId}")
public Order get(@PathVariable String id) { … }     // no {id} in the path: a 500 on every call`, 'java')}</code></pre>
<div class="feature-grid two-col">
  <div class="feature-card">
    <div class="fc-eyebrow">A startup failure</div>
    <div class="fc-title">A route two handlers claim</div>
    <div class="fc-desc">Spring refuses to start — <em>"Ambiguous mapping … there is already … mapped"</em> — found by running the application, a build plus a deploy plus a wait, with no word on which class is new. Both sites are marked. A mapping with no HTTP method collides with every verb: a legacy <code>@RequestMapping</code> beside a new <code>@GetMapping</code>. Two handlers separated by <code>produces</code> are a deliberate pair.</div>
  </div>
  <div class="feature-card">
    <div class="fc-eyebrow">A 500 on every call</div>
    <div class="fc-title">A <code>@PathVariable</code> the path lacks</div>
    <div class="fc-desc"><code>MissingPathVariableException</code>, found by the first user. Invisible to the compiler and to a test that mocks the controller, with the two halves fifteen characters apart. The annotation's name wins: <code>@PathVariable("orderId") String id</code> against <code>/&#123;orderId&#125;</code> is correct.</div>
  </div>
  <div class="feature-card">
    <div class="fc-eyebrow">A misspelling's shape</div>
    <div class="fc-title">A path variable nothing binds</div>
    <div class="fc-desc">Reported only on a handler binding some of the others. One binding none is a deliberate "I do not need it" — reporting it would fire on every <code>/&#123;version&#125;/</code> prefix.</div>
  </div>
  <div class="feature-card">
    <div class="fc-eyebrow">One value, twice</div>
    <div class="fc-title">Two parameters binding one variable</div>
    <div class="fc-desc">Both receive the same value.</div>
  </div>
</div>
<Callout variant="info" title="What is not judged">
  A path built from a property — <code>@GetMapping("&#36;&#123;api.base&#125;/&#123;id&#125;")</code> — has a template Bennu cannot resolve, so nothing is claimed either way.
  And the open file is always read from the <strong>buffer</strong>, not the last scan: a squiggle placed at an offset from three edits ago lands on the wrong line.
</Callout>
