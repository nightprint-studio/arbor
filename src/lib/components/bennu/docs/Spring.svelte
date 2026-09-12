<script lang="ts">
  /**
   * Spring: the wiring in the gutter, keys and names inside annotation strings, bean XML, configuration properties, the
   * YAML side, property files, the Endpoints and bean panels, proxy annotations that do nothing, transactions held across
   * network calls, and endpoints checked against each other.
   */
  import Callout from '$lib/components/shared/ui/Callout.svelte';
  import { highlightCode } from '$lib/utils/highlight';
</script>

<span class="eyebrow">Java &amp; JSP</span>
<h1>Spring</h1>

<p class="doc-lead">
  In a Spring application a large part of the behaviour is declared rather than written — in annotations, in XML, in
  <code>application.yml</code>. Bennu reads that as part of the program, and puts it where you are already looking.
</p>
<p>
  It switches on for a project that uses Spring, detected like every other capability from the pom and the sources. Nothing on this page appears on a
  project without Spring.
</p>

<h2>In the gutter</h2>
<table>
  <thead><tr><th>Icon</th><th>Beside</th><th>Click to</th></tr></thead>
  <tbody>
    <tr><td><code>◆</code></td><td>A bean declaration</td><td>See where it is injected</td></tr>
    <tr><td><code>→</code></td><td>An injection point</td><td>See the beans that can satisfy it</td></tr>
    <tr><td><code>»</code></td><td>A request handler</td><td>See its route</td></tr>
  </tbody>
</table>
<p>
  Injection points include the ones with no constructor in sight: a single constructor needs no <code>@Autowired</code>, and Lombok's generated one makes
  the <code>final</code> fields themselves the injection points.
</p>
<Callout variant="tip" title="A jump with several destinations asks">
  A bean injected in six places, a key read from three — Bennu asks instead of picking. The menu opens at the pointer for a gutter icon and at the caret
  for <kbd>Ctrl</kbd> + <kbd>B</kbd>, and each entry says what kind of site it is, which tells two injections of one bean apart.
</Callout>

<h2>Keys and names inside annotation strings</h2>
<pre><code>{@html highlightCode(`@Value("\${app.timeout:30}")
private int timeout;

@ConditionalOnProperty(name = "app.routing.mode")
@ConditionalOnClass(name = "com.zaxxer.hikari.HikariDataSource")
@ConfigurationProperties(prefix = "app.http.client")`, 'java')}</code></pre>
<p>
  To Java each of those is an opaque string; to Spring they are keys, defaults, SpEL, beans and classes. Bennu reads them as what they are:
</p>
<dl class="meta-grid">
  <dt><code>@Value("$&#123;app.timeout:30&#125;")</code></dt>
  <dd>The key, the default and any embedded SpEL are coloured apart. <kbd>Ctrl</kbd> + <kbd>B</kbd> on the key opens the <code>application*.yml</code> line declaring it, hover shows the value it resolves to, and typing inside <code>$&#123;</code> completes from the project's own keys.</dd>
  <dt>A key without braces</dt>
  <dd><code>@ConditionalOnProperty(name = "app.routing.mode")</code> names a property as directly as <code>$&#123;…&#125;</code> does, so it gets the same colour, hover and go-to — two ways of writing one thing should not look like two. <code>havingValue</code> keeps the string colour: it is what the key is compared <em>to</em>.</dd>
  <dt><code>@ConfigurationProperties(prefix)</code></dt>
  <dd>Coloured as a key. Hover says which property files declare anything under it — a prefix matching nothing is a class binding its defaults for ever, and looks exactly like one that works — and <kbd>Ctrl</kbd> + <kbd>B</kbd> opens the block, one entry per file.</dd>
  <dt>A bean as a string</dt>
  <dd><code>@Qualifier("fast")</code>, <code>@DependsOn("audit")</code>, <code>@Resource(name = "ds")</code> and a SpEL <code>@beanName</code> are the same thing said differently: one colour, and they follow to the declaration.</dd>
  <dt>A class as a string</dt>
  <dd><code>@ConditionalOnClass(name = "…")</code> names a type the only way it can — it may not be on the compile classpath, which is the point of the condition — so a typo silently turns the condition off for ever. It is <strong>coloured</strong> as a type, <strong>completed</strong> from the classpath (the index behind "Import class", JDK and dependencies included), and <kbd>Ctrl</kbd> + <kbd>B</kbd> opens it. <code>@ConditionalOnMissingClass</code> likewise.</dd>
</dl>
<Callout variant="info" title="A bean name is not a class name">
  <code>@ConditionalOnBean(name = "…")</code> is left alone: it names a bean, not a type, however much the two look alike.
</Callout>
<Callout variant="tip" title="Which application.yml answers">
  A project has several — a base file, one per profile, one per module — and which runs is a launch argument, not something the sources say. Open
  <strong>Spring configuration</strong> from the palette and pick one under <em>Resolve against</em>; it is remembered per project. With none set, the
  profile-less files answer — what Spring always loads.
</Callout>

<h2>Bean XML</h2>
<p>
  <code>&lt;bean class=&gt;</code>, <code>ref=</code> and <code>&lt;property name=&gt;</code> navigate and complete, and a <code>&lt;property&gt;</code> naming
  nothing writable on the bean's class is flagged. Every check here speaks only when it is sure:
</p>
<ul>
  <li>the property check stays quiet on a class extending something outside the project, or carrying a Lombok annotation whose accessors Bennu does not model;</li>
  <li>a missing <code>class=</code> is reported only when its package is one the project declares;</li>
  <li>an unknown <code>ref=</code> only when it looks like a typo of a bean that exists — a bean can legitimately come from a jar;</li>
  <li>an unresolved <code>$&#123;key&#125;</code> only when it has no default and the project already configures other keys in that namespace.</li>
</ul>

<h2>An annotation is checked by origin, not by name</h2>
<p>
  <code>@Service</code> is not a reserved word; a project can declare its own, and several do. Each annotation is resolved through the file's imports as the
  compiler would: a qualified use decides outright, then an explicit <code>import</code>, then a wildcard import of the expected package — and a bare name with
  no import can only be a class of the same package, so it is not Spring's. Your <code>com.acme.Service</code> declares no bean, gets no gutter icon and is
  counted in no panel.
</p>
<Callout variant="warning" title="Meta-annotations are not recognised">
  Your own <code>@MyService</code>, itself annotated <code>@Service</code>, is a real stereotype and is missed — which loses a bean rather than inventing one.
</Callout>

<h2>Configuration properties</h2>
<p>
  Hovering a field of a <code>@ConfigurationProperties</code> class shows <em>the full key it binds</em> — the string that appears nowhere in the source, assembled
  from the prefix, the chain of field names above it, and Spring's relaxed binding:
</p>
<table>
  <thead><tr><th>Field</th><th>Binds</th></tr></thead>
  <tbody>
    <tr><td><code>readTimeout</code>, three levels down</td><td><code>app.http.client.read-timeout</code> — with its current value; <kbd>Ctrl</kbd> + <kbd>B</kbd> opens the line setting it</td></tr>
    <tr><td>A <code>Map</code></td><td><code>…«key»…</code></td></tr>
    <tr><td>A <code>List</code></td><td><code>…[0]…</code></td></tr>
    <tr><td>A field with <code>@Name</code></td><td>The name it gives</td></tr>
  </tbody>
</table>
<p>
  A class reached from two roots shows both keys. A class can also be written <em>from</em> the keys — see <strong>Code templates</strong>.
</p>

<h2>Conditional beans</h2>
<p>
  A bean behind a <code>@ConditionalOn…</code> is a different thing from a bean, so it says which condition gates it — in the Beans panel and on hover.
  <code>@ConditionalOnProperty</code>, <code>OnBean</code>, <code>OnMissingBean</code>, <code>OnClass</code>, <code>OnExpression</code> and the rest are read, and the
  property one goes further: its key is real, so hover shows its value now and <kbd>Ctrl</kbd> + <kbd>B</kbd> opens the line setting it.
</p>

<h2>From the YAML side</h2>
<p>
  In an <code>application*.yml</code>, each key something reads carries a count in the gutter — <code>2</code> means two places read it, and clicking asks which.
  <strong>A key with no mark</strong> is the useful signal: nothing in this project reads it.
</p>
<ul>
  <li>The count includes <code>@Value</code>, <code>@ConditionalOnProperty</code>, <code>@ConfigurationProperties</code> fields and XML
    <code>value="$&#123;…&#125;"</code>, and both spellings of a relaxed key — <code>readTimeout</code> and <code>read-timeout</code> — count as one.</li>
  <li>A key whose value is a <strong>list</strong> counts like any other: <code>allowed-origins:</code> with entries under it is what a <code>List&lt;String&gt;</code>
    binds. The items are not keys — <code>servers[0].url</code> is not <code>servers.url</code>.</li>
  <li>The counts follow the code: a save that changes what reads a key refreshes them shortly after, and a rename touching hundreds of files refreshes them once.</li>
</ul>

<h2>Writing a property file</h2>
<dl class="meta-grid">
  <dt>Completion</dt>
  <dd>Every key Spring and the project's libraries document, <em>and</em> the project's own <code>@ConfigurationProperties</code> paths — the half that matters on a legacy tree, where nobody documented your own namespace. Under a nested mapping candidates are relative: <code>u</code> under <code>spring: datasource:</code> completes to <code>url</code>. Values complete where the set is closed: an enum, a boolean, a log level.</dd>
  <dt>Where the vocabulary comes from</dt>
  <dd>Every Spring starter packages a description of the properties it accepts inside its jar, so the list is version-exact, covers third-party and in-house starters, and needs no network. A curated table stands in until dependencies have resolved once. <strong>Spring property reference</strong> in the palette browses it all, marking the keys this project sets.</dd>
  <dt>Ghost text</dt>
  <dd>Only where the answer is single: a documented default for an empty key — <code>server.port:</code> proposes <code>8080</code> — or a prefix one known key continues. <kbd>Tab</kbd> accepts. Never in the middle of a finished key or before an existing value.</dd>
  <dt>Hover</dt>
  <dd>The type, the default, the library's description, and who reads it. The type comes from the declaration, or from the readers — <code>30</code> against a <code>Duration</code> means thirty seconds. Two readers disagreeing about the type are reported, since that is usually a bug. A <code>$&#123;…&#125;</code> in a value is coloured as inside a <code>@Value</code>.</dd>
</dl>

<h3>As an environment variable</h3>
<p>
  Right-click a property line → <em>Show as environment variable</em>. Nothing is written; you get the name and a ready-to-paste line for a <code>.env</code>, a shell,
  <code>docker run</code> and a compose file.
</p>
<table>
  <thead><tr><th>Property</th><th>Variable</th></tr></thead>
  <tbody>
    <tr><td><code>spring.jpa.show-sql</code></td><td><code>SPRING_JPA_SHOWSQL</code></td></tr>
  </tbody>
</table>
<Callout variant="info" title="Dashes are removed, not replaced">
  The one rule everybody forgets — which is why it is worth computing rather than typing.
</Callout>

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
<p>
  <strong>Export</strong> — the ⭳ in the header — takes the list out as CSV, JSON or a Markdown table, to the clipboard or a file: what is on screen, filter and grouping
  applied, with each route's parameters flattened onto its row.
</p>

<h2>The panels</h2>
<table>
  <thead><tr><th>Panel</th><th>Lists</th></tr></thead>
  <tbody>
    <tr><td><strong>Spring beans</strong> <kbd>Ctrl</kbd> + <kbd>Shift</kbd> + <kbd>B</kbd></td><td>Every bean, with its stereotype, scope and profile</td></tr>
    <tr><td><strong>Spring configuration</strong></td><td>Every property key, with its value and source file</td></tr>
    <tr><td><strong>Spring bound properties</strong></td><td>What each <code>@ConfigurationProperties</code> field binds</td></tr>
    <tr><td><strong>Spring property reference</strong></td><td>Everything the dependencies accept, set or not</td></tr>
    <tr><td><strong>Spring beans from libraries</strong></td><td>The beans declared inside the dependencies you name under <strong>Settings → Spring</strong> — by group id, artifact id or a prefix — read from bytecode, one row per artifact</td></tr>
  </tbody>
</table>
<p>All but Endpoints open from the command palette, and each row opens its declaration.</p>
<Callout variant="warning" title="A library's beans are declarations, not facts">
  Boot's auto-configuration is gated — <code>@ConditionalOnMissingBean</code>, <code>@ConditionalOnClass</code>, <code>@ConditionalOnProperty</code> — so a bean in a
  jar is what Spring <em>may</em> register. Each is shown with its conditions, and none takes part in autowiring, completion or any diagnostic: your own beans stay the
  only answer to what this application has.
</Callout>
<p>
  Each panel is offered only where it has something to say: Spring on the classpath turns the tooling on, and what the model found decides which panels exist. A batch
  job with no request mappings gets no Endpoints button, no <kbd>Alt</kbd> + <kbd>4</kbd> and no palette entry. Panels appear as the index finishes, and again after a
  rebuild finds the first route.
</p>

<h2>When an annotation on a method does nothing</h2>
<p>
  <code>@Transactional</code>, <code>@Async</code>, <code>@Cacheable</code> and the authorization annotations are not implemented by the method they are on, but by a
  <strong>proxy</strong> in front of the bean — and anything reaching the bean without the proxy gets the bare method:
</p>
<pre><code>{@html highlightCode(`@Service
public class OrderService {
    public void placeAll(List<Order> orders) {
        for (Order order : orders) {
            place(order);          // through "this", not the proxy: no transaction
        }
    }

    @Transactional
    public void place(Order order) { … }
}`, 'java')}</code></pre>
<div class="feature-grid">
  <div class="feature-card">
    <div class="fc-eyebrow">Reported on the call</div>
    <div class="fc-title">Self-invocation</div>
    <div class="fc-desc">A call from another method of the same class. It is reported on the call — the line that is wrong, and the one carrying no annotation to draw the eye.</div>
  </div>
  <div class="feature-card">
    <div class="fc-eyebrow">Ignored by proxy AOP</div>
    <div class="fc-title">A non-public method</div>
    <div class="fc-desc">Proxy-based AOP skips it outright.</div>
  </div>
  <div class="feature-card">
    <div class="fc-eyebrow">Cannot be overridden</div>
    <div class="fc-title">A final method or class</div>
    <div class="fc-desc">The proxy cannot override it.</div>
  </div>
</div>
<Callout variant="info" title="Quiet under AspectJ">
  A project weaving with AspectJ — <code>mode = AdviceMode.ASPECTJ</code>, or <code>mode="aspectj"</code> in XML — gets none of this, since all three work there. Nor is
  recursion reported (the first call went through the proxy), or a bare call inside a nested class, which resolves to the nested class's own member.
</Callout>

<h2>A transaction held open across a network call</h2>
<p>
  A <code>@Transactional</code> method borrows a connection from the pool for its whole duration. A <code>RestTemplate</code> call inside one keeps that connection for as
  long as the other server takes to answer:
</p>
<pre><code>{@html highlightCode(`@Transactional
public void confirm(Order order) {
    orders.save(order);
    rest.postForObject(paymentsUrl, order, Receipt.class);   // the connection waits on another server
}`, 'java')}</code></pre>
<Callout variant="warning" title="What you are shown is a slow database">
  On a quiet afternoon this is fine. Under load, ten concurrent calls to a four-second endpoint hold every connection while doing no database work: connection wait up,
  queries queueing, pool saturated, database idle — and the cause is one line that looks like an ordinary call to a collaborator.
</Callout>
<ul>
  <li>A remote call is recognised by the <strong>declared type of the receiver</strong>, never by the method name — <code>execute</code>, <code>send</code> and
    <code>get</code> are on everything — so your own repositories, mappers and services are never mistaken for one, and a Feign client or a generated stub is left alone
    rather than guessed at.</li>
  <li><code>Thread.sleep</code> inside a transaction is the same defect without a server at the other end.</li>
  <li>A propagation that <em>suspends</em> the transaction — <code>NOT_SUPPORTED</code>, <code>NEVER</code> — holds nothing, and is left alone: it is the fix.</li>
</ul>

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
